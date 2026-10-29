//! Splits a long-lived `/proc` sample stream into frames the existing parsers understand.

use crate::ssh::SshSession;
use russh::ChannelMsg;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub const FRAME_MARK: &str = "---TRACELYN---";

pub const PROC_STREAM_SCRIPT: &str = "\
while true; do
echo '---TRACELYN---'
echo '---STAT---'
cat /proc/stat
echo '---MEM---'
cat /proc/meminfo
echo '---NET---'
cat /proc/net/dev
echo '---DISK---'
cat /proc/diskstats
echo '---LOAD---'
cat /proc/loadavg
sleep 1
done
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcFrame {
    pub stat: String,
    pub meminfo: String,
    pub net_dev: String,
    pub diskstats: String,
    pub loadavg: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Stat,
    Mem,
    Net,
    Disk,
    Load,
}

#[derive(Default)]
pub struct FrameAcc {
    in_frame: bool,
    section: Option<Section>,
    stat: String,
    meminfo: String,
    net_dev: String,
    diskstats: String,
    loadavg: String,
}

impl FrameAcc {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the previous frame when a new one starts.
    pub fn push_line(&mut self, line: &str) -> Option<ProcFrame> {
        if line == FRAME_MARK {
            let done = self.finish();
            self.reset();
            self.in_frame = true;
            return done;
        }
        if !self.in_frame {
            return None;
        }
        match line {
            "---STAT---" => self.section = Some(Section::Stat),
            "---MEM---" => self.section = Some(Section::Mem),
            "---NET---" => self.section = Some(Section::Net),
            "---DISK---" => self.section = Some(Section::Disk),
            "---LOAD---" => self.section = Some(Section::Load),
            other => self.push_body(other),
        }
        None
    }

    pub fn finish(&mut self) -> Option<ProcFrame> {
        if !self.in_frame || self.stat.is_empty() {
            return None;
        }
        Some(ProcFrame {
            stat: std::mem::take(&mut self.stat),
            meminfo: std::mem::take(&mut self.meminfo),
            net_dev: std::mem::take(&mut self.net_dev),
            diskstats: std::mem::take(&mut self.diskstats),
            loadavg: std::mem::take(&mut self.loadavg),
        })
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn push_body(&mut self, line: &str) {
        let buf = match self.section {
            Some(Section::Stat) => &mut self.stat,
            Some(Section::Mem) => &mut self.meminfo,
            Some(Section::Net) => &mut self.net_dev,
            Some(Section::Disk) => &mut self.diskstats,
            Some(Section::Load) => &mut self.loadavg,
            None => return,
        };
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(line);
    }
}

/// One follow channel. Aborting the handle drops the channel and frees the permit.
pub async fn spawn_proc_stream(
    session: &SshSession,
    tx: mpsc::Sender<ProcFrame>,
) -> Result<JoinHandle<()>, String> {
    let mut channel = session
        .open_follow_channel()
        .await
        .map_err(|err| err.to_string())?;
    if let Err(err) = channel.exec(false, PROC_STREAM_SCRIPT).await {
        let _ = channel.close().await;
        return Err(err.to_string());
    }
    Ok(tokio::spawn(async move {
        let mut acc = FrameAcc::new();
        let mut buf = String::new();
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    buf.push_str(&String::from_utf8_lossy(&data));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();
                        if line.is_empty() {
                            continue;
                        }
                        if let Some(frame) = acc.push_line(&line) {
                            if tx.send(frame).await.is_err() {
                                let _ = channel.close().await;
                                return;
                            }
                        }
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        if let Some(frame) = acc.finish() {
            let _ = tx.send(frame).await;
        }
        let _ = channel.close().await;
    }))
}

#[cfg(test)]
mod tests {
    use super::super::proc_loadavg::parse_loadavg;
    use super::super::proc_stat::parse_cpu_jiffies;
    use super::*;

    #[test]
    fn splits_frames_for_existing_parsers() {
        let raw = "\
noise
---TRACELYN---
---STAT---
cpu  10 2 3 4 1 0 0 0 0 0
cpu0 1 0 0 1 0 0 0 0 0 0
---MEM---
MemTotal: 1000 kB
MemAvailable: 400 kB
---NET---
Inter-|   Receive
 face |bytes
  lo: 1 0 0 0 0 0 0 0 1 0 0 0 0 0 0 0
  eth0: 100 0 0 0 0 0 0 0 50 0 0 0 0 0 0 0
---DISK---
   8       0 sda 1 0 10 0 1 0 20 0 0 0 0
---LOAD---
0.50 0.25 0.10 1/100 12
---TRACELYN---
---STAT---
cpu  20 2 3 8 1 0 0 0 0 0
";
        let mut acc = FrameAcc::new();
        let mut frames = Vec::new();
        for line in raw.lines() {
            if let Some(frame) = acc.push_line(line) {
                frames.push(frame);
            }
        }
        assert_eq!(frames.len(), 1);
        let frame = &frames[0];
        assert!(parse_cpu_jiffies(&frame.stat).is_some());
        assert_eq!(parse_loadavg(&frame.loadavg), Some((0.5, 0.25, 0.1)));
        assert!(frame.meminfo.contains("MemTotal"));
        assert!(frame.net_dev.contains("eth0"));
        assert!(frame.diskstats.contains("sda"));
        assert!(acc.finish().unwrap().stat.contains("cpu"));
    }
}
