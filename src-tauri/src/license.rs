use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tauri::{Manager, State};

const DAY: i64 = 86400;
const TRIAL: i64 = 14 * DAY;
const GRACE: i64 = 7 * DAY;
const STORAGE_ERROR: &str = "License storage could not be verified. Monitoring remains available. Restore OS credential access or activate a license in Settings.";
pub const READ_ONLY: &str =
    "Your license is read-only. Monitoring remains available. Activate a license in Settings.";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum LicenseState {
    Trial,
    Active,
    Expired,
    Grace,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    trial_start: i64,
    last_seen: i64,
    last_valid: Option<i64>,
    invalid: bool,
    offline: bool,
    expires_at: Option<i64>,
    key: Option<String>,
    instance: Option<String>,
    device: String,
    plan: String,
}
impl Record {
    fn new(now: i64) -> Self {
        Self {
            trial_start: now,
            last_seen: now,
            last_valid: None,
            invalid: false,
            offline: false,
            expires_at: None,
            key: None,
            instance: None,
            device: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This device".into()),
            plan: "Trial".into(),
        }
    }
}
// Pure state machine. Keygen can implement LicenseProvider without changing these rules.
pub fn evaluate(r: &Record, now: i64) -> LicenseState {
    if now + 300 < r.last_seen {
        return LicenseState::Invalid;
    }
    if r.key.is_none() {
        return if now < r.trial_start + TRIAL {
            LicenseState::Trial
        } else {
            LicenseState::Expired
        };
    }
    if r.invalid || r.expires_at.is_some_and(|expiry| now >= expiry) {
        return LicenseState::Invalid;
    }
    match r.last_valid {
        Some(last) if now - last <= GRACE => {
            if r.offline || now - last >= DAY {
                LicenseState::Grace
            } else {
                LicenseState::Active
            }
        }
        _ => LicenseState::Expired,
    }
}
#[derive(Debug)]
pub enum ProviderError {
    Offline,
    Rejected(String),
    Service,
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => write!(f, "Network offline. Check your connection and try again."),
            Self::Service => write!(f, "The license service is unavailable. Try again shortly."),
            Self::Rejected(message) => write!(f, "{message}"),
        }
    }
}
pub struct Validation {
    instance: String,
    plan: String,
    expires_at: Option<i64>,
}
#[async_trait]
pub trait LicenseProvider: Send + Sync {
    async fn activate(&self, key: &str, device: &str) -> Result<Validation, ProviderError>;
    async fn validate(&self, key: &str, instance: &str) -> Result<Validation, ProviderError>;
    async fn deactivate(&self, key: &str, instance: &str) -> Result<(), ProviderError>;
}
struct LemonSqueezy {
    client: reqwest::Client,
}
impl LemonSqueezy {
    async fn request(
        &self,
        action: &str,
        key: &str,
        field: &str,
        value: &str,
    ) -> Result<serde_json::Value, ProviderError> {
        let response = self
            .client
            .post(format!("https://api.lemonsqueezy.com/v1/licenses/{action}"))
            .header("Accept", "application/json")
            .form(&[("license_key", key), (field, value)])
            .send()
            .await
            .map_err(|_| ProviderError::Offline)?;
        if response.status().is_server_error() || response.status().as_u16() == 429 {
            return Err(ProviderError::Service);
        }
        let body: serde_json::Value = response.json().await.map_err(|_| ProviderError::Service)?;
        let success = match action {
            "activate" => "activated",
            "validate" => "valid",
            _ => "deactivated",
        };
        if body[success].as_bool() != Some(true) {
            let status = body["license_key"]["status"].as_str().unwrap_or("");
            let error = body["error"].as_str().unwrap_or("").to_lowercase();
            let message = if status == "expired" || error.contains("expired") {
                "This license key has expired. Renew your subscription."
            } else if status == "disabled" || error.contains("disabled") {
                "This license key is disabled. Contact support."
            } else if error.contains("limit") {
                "Activation limit reached. Deactivate another device and try again."
            } else {
                "Invalid license key or device activation. Check your key and try again."
            };
            return Err(ProviderError::Rejected(message.into()));
        }
        Ok(body)
    }
    fn validation(body: serde_json::Value, fallback: &str) -> Result<Validation, ProviderError> {
        let instance = body["instance"]["id"]
            .as_str()
            .unwrap_or(fallback)
            .to_string();
        if instance.is_empty() {
            return Err(ProviderError::Service);
        }
        // Reject keys from other products. Configure these IDs before selling licenses.
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../license-config.json"))
                .map_err(|_| ProviderError::Service)?;
        for field in ["store_id", "product_id", "variant_id"] {
            if config[field].as_u64().filter(|v| *v > 0).is_none() {
                return Err(ProviderError::Rejected(
                    "Licensing is not configured for this release. Contact support.".into(),
                ));
            }
            if body["meta"][field] != config[field] {
                return Err(ProviderError::Rejected(
                    "This key belongs to a different product or plan.".into(),
                ));
            }
        }
        let expires_at = match body["license_key"]["expires_at"].as_str() {
            Some(value) => Some(
                chrono::DateTime::parse_from_rfc3339(value)
                    .map_err(|_| ProviderError::Service)?
                    .timestamp(),
            ),
            None => None,
        };
        Ok(Validation {
            instance,
            plan: body["meta"]["variant_name"]
                .as_str()
                .unwrap_or("Licensed")
                .into(),
            expires_at,
        })
    }
}
#[async_trait]
impl LicenseProvider for LemonSqueezy {
    async fn activate(&self, key: &str, device: &str) -> Result<Validation, ProviderError> {
        let body = self
            .request("activate", key, "instance_name", device)
            .await?;
        match Self::validation(body.clone(), "") {
            Ok(value) => Ok(value),
            Err(err) => {
                if let Some(id) = body["instance"]["id"].as_str() {
                    let _ = self.deactivate(key, id).await;
                }
                Err(err)
            }
        }
    }
    async fn validate(&self, key: &str, instance: &str) -> Result<Validation, ProviderError> {
        Self::validation(
            self.request("validate", key, "instance_id", instance)
                .await?,
            instance,
        )
    }
    async fn deactivate(&self, key: &str, instance: &str) -> Result<(), ProviderError> {
        self.request("deactivate", key, "instance_id", instance)
            .await
            .map(|_| ())
    }
}
#[derive(Serialize, Deserialize)]
struct Signed {
    data: String,
    mac: String,
}
struct Store {
    path: PathBuf,
    secret: String,
}
fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.tracelyn.app.license", name).map_err(|_| STORAGE_ERROR.into())
}
fn credential(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(STORAGE_ERROR.into()),
    }
}
impl Store {
    fn encode(&self, r: &Record) -> Result<String, String> {
        let data = serde_json::to_string(r).map_err(|_| STORAGE_ERROR)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret.as_bytes()).map_err(|_| STORAGE_ERROR)?;
        mac.update(data.as_bytes());
        serde_json::to_string(&Signed {
            data,
            mac: hex::encode(mac.finalize().into_bytes()),
        })
        .map_err(|_| STORAGE_ERROR.into())
    }
    fn decode(&self, text: &str) -> Result<Record, String> {
        let signed: Signed = serde_json::from_str(text).map_err(|_| STORAGE_ERROR)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret.as_bytes()).map_err(|_| STORAGE_ERROR)?;
        mac.update(signed.data.as_bytes());
        mac.verify_slice(&hex::decode(signed.mac).map_err(|_| STORAGE_ERROR)?)
            .map_err(|_| STORAGE_ERROR)?;
        serde_json::from_str(&signed.data).map_err(|_| STORAGE_ERROR.into())
    }
    fn restore(
        &self,
        marker: Option<&str>,
        file: Option<&str>,
        timestamp: i64,
    ) -> Result<Record, String> {
        let record = match marker {
            Some(value) => self.decode(value)?,
            None if file.is_none() => Record::new(timestamp),
            None => return Err(STORAGE_ERROR.into()),
        };
        if let Some(value) = file {
            self.decode(value)?;
        }
        Ok(record)
    }
    fn load(path: PathBuf) -> Result<(Self, Record), String> {
        let marker = credential("state")?;
        let secret = match credential("hmac")? {
            Some(v) => v,
            None if marker.is_none() && !path.exists() => {
                let v = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
                entry("hmac")?.set_password(&v).map_err(|_| STORAGE_ERROR)?;
                v
            }
            None => return Err(STORAGE_ERROR.into()),
        };
        let store = Self { path, secret };
        // The keyring copy is authoritative. Deleting or replaying the file never resets time.
        let file = match std::fs::read_to_string(&store.path) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(STORAGE_ERROR.into()),
        };
        let record = store.restore(marker.as_deref(), file.as_deref(), now())?;
        store.save(&record)?;
        Ok((store, record))
    }
    fn save(&self, record: &Record) -> Result<(), String> {
        let text = self.encode(record)?;
        entry("state")?
            .set_password(&text)
            .map_err(|_| STORAGE_ERROR)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| STORAGE_ERROR)?;
        }
        std::fs::write(&self.path, text).map_err(|_| STORAGE_ERROR.into())
    }
}
struct Inner {
    record: Record,
    store: Option<Store>,
    error: Option<String>,
}
pub struct LicenseManager {
    path: PathBuf,
    inner: parking_lot::Mutex<Inner>,
    operation: tokio::sync::Mutex<()>,
    provider: Box<dyn LicenseProvider>,
}
static LICENSE: OnceLock<Arc<LicenseManager>> = OnceLock::new();
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn require_write() -> Result<(), String> {
    let read_only = || {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../license-config.json")).unwrap_or_default();
        format!(
            "{} Upgrade at {}",
            READ_ONLY,
            config["buy_url"]
                .as_str()
                .unwrap_or("https://example.com/tracelyn/buy")
        )
    };
    let manager = LICENSE.get().ok_or_else(read_only)?;
    if manager.snapshot().can_write {
        Ok(())
    } else {
        Err(read_only())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    status: LicenseState,
    can_write: bool,
    days_left: i64,
    masked_key: Option<String>,
    device: String,
    plan: String,
    error: Option<String>,
}
impl LicenseManager {
    fn snapshot(&self) -> Snapshot {
        let inner = self.inner.lock();
        let r = &inner.record;
        let status = if inner.error.is_some() {
            LicenseState::Invalid
        } else {
            evaluate(r, now())
        };
        Snapshot {
            status,
            can_write: matches!(
                status,
                LicenseState::Trial | LicenseState::Active | LicenseState::Grace
            ),
            days_left: {
                let deadline = if r.key.is_none() {
                    Some(r.trial_start + TRIAL)
                } else if status == LicenseState::Grace {
                    r.last_valid
                        .map(|last| r.expires_at.unwrap_or(last + GRACE).min(last + GRACE))
                } else {
                    r.expires_at
                };
                deadline
                    .map(|end| ((end - now()).max(0) + DAY - 1) / DAY)
                    .unwrap_or(-1)
            },
            masked_key: r.key.as_ref().map(|key| {
                let chars: Vec<char> = key.chars().collect();
                format!(
                    "{}-****-****-{}",
                    chars.iter().take(4).collect::<String>(),
                    chars
                        .iter()
                        .skip(chars.len().saturating_sub(4))
                        .collect::<String>()
                )
            }),
            device: r.device.clone(),
            plan: r.plan.clone(),
            error: inner.error.clone(),
        }
    }
    fn persist(inner: &mut Inner) -> Result<(), String> {
        inner.record.last_seen = inner.record.last_seen.max(now());
        let result = inner
            .store
            .as_ref()
            .ok_or_else(|| STORAGE_ERROR.to_string())?
            .save(&inner.record);
        inner.error = result.as_ref().err().cloned();
        result
    }
    async fn refresh(&self, force: bool) {
        let _guard = self.operation.lock().await;
        let record = self.inner.lock().record.clone();
        if let (Some(key), Some(instance)) = (&record.key, &record.instance) {
            if force || record.last_valid.is_none_or(|last| now() - last >= DAY) {
                let result = self.provider.validate(key, instance).await;
                let mut inner = self.inner.lock();
                match result {
                    Ok(v) => {
                        inner.record.last_valid = Some(now());
                        inner.record.invalid = false;
                        inner.record.offline = false;
                        inner.record.expires_at = v.expires_at;
                        inner.record.plan = v.plan;
                    }
                    Err(ProviderError::Rejected(_)) => {
                        inner.record.invalid = true;
                    }
                    Err(_) => {
                        inner.record.offline = true;
                    }
                }
            }
        }
        let _ = Self::persist(&mut self.inner.lock());
    }
}
pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("license.json");
    let (store, record, error) = match Store::load(path.clone()) {
        Ok((s, r)) => (Some(s), r, None),
        Err(e) => (None, Record::new(now()), Some(e)),
    };
    let manager = Arc::new(LicenseManager {
        path,
        inner: parking_lot::Mutex::new(Inner {
            store,
            record,
            error,
        }),
        operation: tokio::sync::Mutex::new(()),
        provider: Box::new(LemonSqueezy {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())?,
        }),
    });
    let _ = LICENSE.set(manager.clone());
    app.manage(manager.clone());
    tauri::async_runtime::spawn(async move {
        manager.refresh(true).await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            manager.refresh(false).await;
        }
    });
    Ok(())
}
#[tauri::command]
pub fn license_status(manager: State<Arc<LicenseManager>>) -> Snapshot {
    manager.snapshot()
}
#[tauri::command]
pub async fn license_activate(
    manager: State<'_, Arc<LicenseManager>>,
    key: String,
) -> Result<Snapshot, String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 256 {
        return Err("Enter a valid license key.".into());
    }
    let _guard = manager.operation.lock().await;
    let record = manager.inner.lock().record.clone();
    if record.key.is_some() {
        return Err("Deactivate this device before entering another key.".into());
    }
    // Verify keyring access before consuming an activation slot.
    let recovery = if manager.inner.lock().store.is_none() {
        let secret = credential("hmac")?
            .unwrap_or_else(|| format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()));
        entry("hmac")?
            .set_password(&secret)
            .map_err(|_| STORAGE_ERROR)?;
        Some(Store {
            path: manager.path.clone(),
            secret,
        })
    } else {
        None
    };
    let v = manager
        .provider
        .activate(key, &record.device)
        .await
        .map_err(|e| e.to_string())?;
    let mut inner = manager.inner.lock();
    // Online activation can recover damaged storage without restarting the trial.
    if let Some(store) = recovery {
        inner.store = Some(store);
        inner.record.trial_start = 0;
    }
    inner.record.key = Some(key.into());
    inner.record.instance = Some(v.instance);
    inner.record.last_valid = Some(now());
    inner.record.expires_at = v.expires_at;
    inner.record.invalid = false;
    inner.record.offline = false;
    inner.record.plan = v.plan;
    LicenseManager::persist(&mut inner)?;
    drop(inner);
    Ok(manager.snapshot())
}
#[tauri::command]
pub async fn license_deactivate(
    manager: State<'_, Arc<LicenseManager>>,
) -> Result<Snapshot, String> {
    let _guard = manager.operation.lock().await;
    let r = manager.inner.lock().record.clone();
    if let (Some(key), Some(instance)) = (&r.key, &r.instance) {
        match manager.provider.deactivate(key, instance).await {
            Ok(()) => {}
            Err(ProviderError::Rejected(_))
                if r.invalid || r.expires_at.is_some_and(|expiry| now() >= expiry) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut inner = manager.inner.lock();
    inner.record.key = None;
    inner.record.instance = None;
    inner.record.last_valid = None;
    inner.record.invalid = false;
    inner.record.plan = "Trial".into();
    inner.record.expires_at = None;
    LicenseManager::persist(&mut inner)?;
    drop(inner);
    Ok(manager.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trial_boundaries() {
        let r = Record::new(1000);
        assert_eq!(evaluate(&r, 1000), LicenseState::Trial);
        assert_eq!(evaluate(&r, 1000 + TRIAL - 1), LicenseState::Trial);
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Expired);
    }
    #[test]
    fn active_offline_grace_and_recovery() {
        let mut r = Record::new(1000);
        r.key = Some("key".into());
        r.last_valid = Some(1000);
        assert_eq!(evaluate(&r, 1000), LicenseState::Active);
        r.offline = true;
        assert_eq!(evaluate(&r, 1001), LicenseState::Grace);
        assert_eq!(evaluate(&r, 1000 + GRACE), LicenseState::Grace);
        assert_eq!(evaluate(&r, 1001 + GRACE), LicenseState::Expired);
        r.last_valid = Some(1001 + GRACE);
        r.offline = false;
        assert_eq!(evaluate(&r, 1001 + GRACE), LicenseState::Active);
        r.invalid = true;
        assert_eq!(evaluate(&r, 1001 + GRACE), LicenseState::Invalid);
    }
    #[test]
    fn revoked_expired_and_clock_rollback() {
        let mut r = Record::new(1000);
        assert_eq!(evaluate(&r, 699), LicenseState::Invalid);
        r.key = Some("key".into());
        r.last_valid = Some(1000);
        r.expires_at = Some(1001);
        assert_eq!(evaluate(&r, 1001), LicenseState::Invalid);
        r.key = None;
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Expired);
    }
    #[test]
    fn signed_storage_detects_edits() {
        let store = Store {
            path: PathBuf::new(),
            secret: "test-secret".into(),
        };
        let text = store.encode(&Record::new(1000)).unwrap();
        assert_eq!(store.decode(&text).unwrap().trial_start, 1000);
        assert!(store.decode(&text.replace("1000", "2000")).is_err());
    }
    #[test]
    fn file_deletion_and_replay_never_restart_trial() {
        let store = Store {
            path: PathBuf::new(),
            secret: "test-secret".into(),
        };
        let first = Record::new(1000);
        let old_file = store.encode(&first).unwrap();
        let mut current = first;
        current.last_seen = 1000 + TRIAL;
        let marker = store.encode(&current).unwrap();
        for file in [None, Some(old_file.as_str())] {
            let restored = store.restore(Some(&marker), file, 1000 + TRIAL).unwrap();
            assert_eq!(restored.trial_start, 1000);
            assert_eq!(restored.last_seen, 1000 + TRIAL);
            assert_eq!(evaluate(&restored, 1000 + TRIAL), LicenseState::Expired);
        }
        assert!(store.restore(None, Some(&old_file), 1000 + TRIAL).is_err());
        assert!(store
            .restore(Some(&marker), Some("corrupt"), 1000 + TRIAL)
            .is_err());
    }
    #[test]
    fn activation_deactivation_and_missing_validation() {
        let mut r = Record::new(1000);
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Expired);
        r.key = Some("key".into());
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Expired);
        r.last_valid = Some(1000 + TRIAL);
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Active);
        assert_eq!(evaluate(&r, 1000 + TRIAL + DAY), LicenseState::Grace);
        r.invalid = true;
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Invalid);
        r.invalid = false;
        r.key = None;
        assert_eq!(evaluate(&r, 1000 + TRIAL), LicenseState::Expired);
    }
    #[test]
    fn updater_configuration() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["bundle"]["createUpdaterArtifacts"], false);
        let release: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.release.conf.json")).unwrap();
        assert_eq!(release["bundle"]["createUpdaterArtifacts"], true);
        assert!(
            config["plugins"]["updater"]["pubkey"]
                .as_str()
                .unwrap()
                .len()
                > 10
        );
        let endpoint = config["plugins"]["updater"]["endpoints"][0]
            .as_str()
            .unwrap();
        assert!(endpoint.starts_with("https://github.com/"));
        assert!(endpoint.ends_with("/releases/latest/download/latest.json"));
        assert!(config["bundle"]["windows"]["certificateThumbprint"].is_null());
        let capabilities: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let permissions = capabilities["permissions"].as_array().unwrap();
        assert!(permissions.contains(&serde_json::json!("updater:default")));
        assert!(permissions.contains(&serde_json::json!("process:allow-restart")));
    }
}
