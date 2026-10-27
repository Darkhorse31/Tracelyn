use crate::config::ServerRecord;
use crate::ops::plan::{needs_sudo_stdin, redact};
use crate::ssh::{connect_persistent, HostKeyStore, SshChannel, SshSession};
use base64::Engine;
use russh::ChannelMsg;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

const MAX_TABS_PER_SERVER: usize = 8;
const OUTPUT_CAP: usize = 8192;

#[derive(Clone)]
pub struct OpsRuntime {
    inner: Arc<Inner>,
}

struct Inner {
    sessions: Mutex<HashMap<String, Arc<Mutex<SshSession>>>>,
    tabs: Mutex<HashMap<String, TabSlot>>,
}

struct TabSlot {
    server_id: String,
    tx: mpsc::Sender<TermCmd>,
}

enum TermCmd {
    Data(Vec<u8>),
    Resize(u32, u32),
    Close,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TermEvent {
    data_b64: String,
    closed: bool,
    error: Option<String>,
}

pub struct ExecOut {
    pub exit_code: i32,
    pub output: String,
}

impl OpsRuntime {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                sessions: Mutex::new(HashMap::new()),
                tabs: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn session_for(
        &self,
        server: &ServerRecord,
        host_keys: Arc<Mutex<HostKeyStore>>,
    ) -> Result<Arc<Mutex<SshSession>>, String> {
        if let Some(existing) = self.inner.sessions.lock().await.get(&server.id).cloned() {
            if !existing.lock().await.is_closed().await {
                return Ok(existing);
            }
        }
        let fresh = connect_persistent(server, host_keys)
            .await
            .map_err(|err| err.to_string())?;
        let arc = Arc::new(Mutex::new(fresh));
        let mut map = self.inner.sessions.lock().await;
        if let Some(existing) = map.get(&server.id).cloned() {
            if !existing.lock().await.is_closed().await {
                return Ok(existing);
            }
        }
        // ponytail: a second concurrent connect can orphan the loser; interactive use never overlaps.
        map.insert(server.id.clone(), arc.clone());
        Ok(arc)
    }

    pub async fn drop_server(&self, server_id: &str) {
        let tabs: Vec<TabSlot> = {
            let mut tabs = self.inner.tabs.lock().await;
            let ids: Vec<String> = tabs
                .iter()
                .filter(|(_, slot)| slot.server_id == server_id)
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter().filter_map(|id| tabs.remove(&id)).collect()
        };
        for slot in tabs {
            let _ = slot.tx.send(TermCmd::Close).await;
        }
        if let Some(session) = self.inner.sessions.lock().await.remove(server_id) {
            let guard = session.lock().await;
            let _ = guard
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }

    pub async fn close_all(&self) {
        let ids: Vec<String> = self.inner.tabs.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.close_tab(&id).await;
        }
        let sessions: Vec<_> = self
            .inner
            .sessions
            .lock()
            .await
            .drain()
            .map(|(_, session)| session)
            .collect();
        for session in sessions {
            let guard = session.lock().await;
            let _ = guard
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }

    pub async fn open_terminal(
        &self,
        app: AppHandle,
        server: &ServerRecord,
        host_keys: Arc<Mutex<HostKeyStore>>,
        cols: u32,
        rows: u32,
    ) -> Result<String, String> {
        let cols = cols.clamp(1, 500);
        let rows = rows.clamp(1, 500);
        let session = self.session_for(server, host_keys).await?;
        {
            let tabs = self.inner.tabs.lock().await;
            let open = tabs.values().filter(|slot| slot.server_id == server.id).count();
            if open >= MAX_TABS_PER_SERVER {
                return Err("Close a terminal tab first (8 max).".into());
            }
        }
        let channel = {
            let guard = session.lock().await;
            guard
                .channel_open_session()
                .await
                .map_err(|err| err.to_string())?
        };
        let modes: [(russh::Pty, u32); 0] = [];
        channel
            .request_pty(false, "xterm-256color", cols, rows, 0, 0, &modes)
            .await
            .map_err(|err| err.to_string())?;
        channel
            .request_shell(false)
            .await
            .map_err(|err| err.to_string())?;

        let (tx, rx) = mpsc::channel(32);
        let tab_id = uuid::Uuid::new_v4().to_string();
        self.inner.tabs.lock().await.insert(
            tab_id.clone(),
            TabSlot {
                server_id: server.id.clone(),
                tx,
            },
        );
        let inner = self.inner.clone();
        let tab_for_task = tab_id.clone();
        tauri::async_runtime::spawn(async move {
            pump_terminal(app, inner, tab_for_task, channel, rx).await;
        });
        Ok(tab_id)
    }

    pub async fn write_tab(&self, tab_id: &str, data: &str) -> Result<(), String> {
        if data.len() > 65_536 {
            return Err("Terminal input is too large.".into());
        }
        let tx = self
            .inner
            .tabs
            .lock()
            .await
            .get(tab_id)
            .map(|slot| slot.tx.clone());
        let Some(tx) = tx else {
            return Err("Terminal closed.".into());
        };
        tx.send(TermCmd::Data(data.as_bytes().to_vec()))
            .await
            .map_err(|_| "Terminal closed.".to_string())
    }

    pub async fn resize_tab(&self, tab_id: &str, cols: u32, rows: u32) -> Result<(), String> {
        let tx = self
            .inner
            .tabs
            .lock()
            .await
            .get(tab_id)
            .map(|slot| slot.tx.clone());
        let Some(tx) = tx else {
            return Err("Terminal closed.".into());
        };
        tx.send(TermCmd::Resize(cols.clamp(1, 500), rows.clamp(1, 500)))
            .await
            .map_err(|_| "Terminal closed.".to_string())
    }

    pub async fn close_tab(&self, tab_id: &str) -> Result<(), String> {
        let slot = self.inner.tabs.lock().await.remove(tab_id);
        let Some(slot) = slot else {
            return Ok(());
        };
        let _ = slot.tx.send(TermCmd::Close).await;
        Ok(())
    }
}

pub async fn run_shell(
    session: &Mutex<SshSession>,
    command: &str,
    stdin_secret: Option<&str>,
    timeout: Duration,
) -> Result<ExecOut, String> {
    let secret = stdin_secret.map(str::to_string);
    match tokio::time::timeout(timeout, exec_channel(session, command, secret.clone())).await {
        Err(_) => Ok(ExecOut {
            exit_code: -1,
            output: "timed out".into(),
        }),
        Ok(result) => result,
    }
}

async fn exec_channel(
    session: &Mutex<SshSession>,
    command: &str,
    secret: Option<String>,
) -> Result<ExecOut, String> {
    let mut channel = {
        let guard = session.lock().await;
        guard
            .channel_open_session()
            .await
            .map_err(|err| err.to_string())?
    };
    channel
        .exec(true, command.to_string())
        .await
        .map_err(|err| err.to_string())?;
    if let Some(secret) = secret.as_deref() {
        if needs_sudo_stdin(command) {
            let mut bytes = secret.as_bytes().to_vec();
            if !bytes.ends_with(b"\n") {
                bytes.push(b'\n');
            }
            channel
                .data(Cursor::new(bytes))
                .await
                .map_err(|err| err.to_string())?;
        }
    }
    let _ = channel.eof().await;
    let mut output = String::new();
    let mut exit_code = -1;
    let mut saw_exit = false;
    let mut saw_eof = false;
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => push_limited(&mut output, &data),
            Some(ChannelMsg::ExtendedData { data, .. }) => push_limited(&mut output, &data),
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = exit_status as i32;
                saw_exit = true;
                if saw_eof {
                    break;
                }
            }
            Some(ChannelMsg::Eof) => {
                saw_eof = true;
                if saw_exit {
                    break;
                }
            }
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    Ok(ExecOut {
        exit_code,
        output: redact(&output, secret.as_deref()),
    })
}

fn push_limited(output: &mut String, data: &[u8]) {
    if output.len() >= OUTPUT_CAP {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let room = OUTPUT_CAP - output.len();
    if text.len() <= room {
        output.push_str(&text);
    } else {
        let end = text.floor_char_boundary(room);
        output.push_str(&text[..end]);
    }
}

async fn pump_terminal(
    app: AppHandle,
    inner: Arc<Inner>,
    tab_id: String,
    mut channel: SshChannel,
    mut rx: mpsc::Receiver<TermCmd>,
) {
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    None | Some(TermCmd::Close) => {
                        let _ = channel.close().await;
                        break;
                    }
                    Some(TermCmd::Data(bytes)) => {
                        if channel.data(Cursor::new(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Some(TermCmd::Resize(cols, rows)) => {
                        let _ = channel.window_change(cols, rows, 0, 0).await;
                    }
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => emit_bytes(&app, &tab_id, &data),
                    Some(ChannelMsg::ExtendedData { data, .. }) => emit_bytes(&app, &tab_id, &data),
                    Some(ChannelMsg::Eof)
                    | Some(ChannelMsg::Close)
                    | Some(ChannelMsg::ExitStatus { .. })
                    | None => break,
                    _ => {}
                }
            }
        }
    }
    inner.tabs.lock().await.remove(&tab_id);
    let _ = app.emit(
        &format!("ops-terminal:{tab_id}"),
        TermEvent {
            data_b64: String::new(),
            closed: true,
            error: None,
        },
    );
}

fn emit_bytes(app: &AppHandle, tab_id: &str, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    for chunk in data.chunks(16 * 1024) {
        let _ = app.emit(
            &format!("ops-terminal:{tab_id}"),
            TermEvent {
                data_b64: base64::engine::general_purpose::STANDARD.encode(chunk),
                closed: false,
                error: None,
            },
        );
    }
}
