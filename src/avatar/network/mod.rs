use std::{
    collections::HashMap,
    fs, io,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use uuid::Uuid;

use crate::config;

pub const DEFAULT_ENDPOINT: &str = "https://figura.moonlight-devs.org/";

pub const PING_PACKET_ID: u8 = 0x01;


pub mod paths {
    pub const USERDATA: &str = "api/user";
    pub const AVATARS: &str = "api/avatar";
    pub const EQUIP: &str = "api/equip";
}

#[derive(Clone, Debug, Default)]
pub struct Userdata {
    pub uuid: Uuid,
    pub username: String,
    pub equipped_hash: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EquippedAvatar {
    pub owner: Uuid,
    pub hash: String,
    pub name: String,
    pub download_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PingMessage {
    pub id: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct RemotePlayerAvatar {
    pub player: Uuid,
    pub hash: String,
    pub name: String,
    pub cached: bool,
}

#[derive(Default)]
struct ClientState {
    endpoint: String,
    auth_token: Option<String>,
    cache_dir: Option<PathBuf>,
    connected: bool,
    mode: BackendMode,
    last_status: String,
    mock_store: HashMap<String, Vec<u8>>,
    equipped: HashMap<Uuid, EquippedAvatar>,
    remotes: HashMap<Uuid, RemotePlayerAvatar>,
    backoff_until: Option<Instant>,
    reconnect_attempts: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackendMode {
    #[default]
    Mock,
    Live,
}

fn state() -> &'static Mutex<ClientState> {
    static STATE: OnceLock<Mutex<ClientState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(ClientState {
            endpoint: config::figura_endpoint().unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            ..Default::default()
        })
    })
}

pub fn status() -> String {
    state()
        .lock()
        .map(|guard| {
            let mode = match guard.mode {
                BackendMode::Mock => "mock",
                BackendMode::Live => "live",
            };
            let link = if guard.connected {
                "connected"
            } else {
                "offline"
            };
            if guard.last_status.is_empty() {
                format!("{link}/{mode} @ {}", guard.endpoint)
            } else {
                format!("{link}/{mode}: {}", guard.last_status)
            }
        })
        .unwrap_or_else(|_| "network poisoned".to_string())
}

pub fn set_endpoint(endpoint: impl Into<String>) {
    if let Ok(mut guard) = state().lock() {
        guard.endpoint = normalize_endpoint(endpoint.into());
        guard.last_status = format!("endpoint set: {}", guard.endpoint);
        let _ = config::set_figura_endpoint(&guard.endpoint);
    }
}

pub fn endpoint() -> String {
    state()
        .lock()
        .map(|guard| guard.endpoint.clone())
        .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string())
}

pub fn set_auth_token(token: Option<String>) {
    if let Ok(mut guard) = state().lock() {
        guard.auth_token = token;
        guard.last_status = if guard.auth_token.is_some() {
            "auth token stored in memory".to_string()
        } else {
            "auth token cleared".to_string()
        };
    }
}

pub fn auth_token() -> Option<String> {
    state()
        .lock()
        .ok()
        .and_then(|guard| guard.auth_token.clone())
}

pub fn clear_auth_token() {
    set_auth_token(None);
}

pub fn cache_dir() -> io::Result<PathBuf> {
    let mut guard = state()
        .lock()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "network poisoned"))?;
    if let Some(path) = guard.cache_dir.clone() {
        return Ok(path);
    }
    let root = config::ensure_root_dir()?;
    let path = root.join("figura_cache");
    fs::create_dir_all(&path)?;
    guard.cache_dir = Some(path.clone());
    Ok(path)
}

pub fn is_connected() -> bool {
    state().lock().map(|guard| guard.connected).unwrap_or(false)
}

pub fn backend_mode() -> BackendMode {
    state().lock().map(|guard| guard.mode).unwrap_or_default()
}


pub fn connect_mock() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| "network poisoned".to_string())?;
    if let Some(until) = guard.backoff_until {
        if Instant::now() < until {
            return Err("reconnect backoff active".into());
        }
    }
    guard.mode = BackendMode::Mock;
    guard.connected = true;
    guard.reconnect_attempts = 0;
    guard.backoff_until = None;
    guard.last_status = "mock backend connected".to_string();
    Ok(())
}



pub fn connect_live() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| "network poisoned".to_string())?;
    if guard.auth_token.is_none() {
        guard.mode = BackendMode::Live;
        guard.connected = false;
        guard.last_status = "live mode needs in-memory auth token from JVM session".to_string();
        return Err(guard.last_status.clone());
    }
    guard.mode = BackendMode::Live;
    guard.connected = true;
    guard.reconnect_attempts = 0;
    guard.backoff_until = None;
    guard.last_status = format!("live connected to {}", guard.endpoint);
    Ok(())
}

pub fn disconnect() {
    if let Ok(mut guard) = state().lock() {
        guard.connected = false;
        guard.auth_token = None;
        guard.last_status = "disconnected; auth cleared".to_string();
    }
}

pub fn note_reconnect_failure() {
    if let Ok(mut guard) = state().lock() {
        guard.connected = false;
        guard.reconnect_attempts = guard.reconnect_attempts.saturating_add(1);
        let secs = (1u64 << guard.reconnect_attempts.min(5)).min(60);
        guard.backoff_until = Some(Instant::now() + Duration::from_secs(secs));
        guard.last_status = format!("reconnect backoff {secs}s");
    }
}

pub trait FiguraBackend {
    fn discover_equipped(&self, user: Uuid) -> Result<Vec<EquippedAvatar>, String>;
    fn download_avatar(&self, hash: &str) -> Result<Vec<u8>, String>;
    fn upload_avatar(&self, bytes: &[u8]) -> Result<String, String>;
    fn send_ping(&self, message: &PingMessage) -> Result<(), String>;
}

struct ActiveBackend;

impl FiguraBackend for ActiveBackend {
    fn discover_equipped(&self, user: Uuid) -> Result<Vec<EquippedAvatar>, String> {
        let guard = state().lock().map_err(|_| "network poisoned".to_string())?;
        if !guard.connected {
            return Err("not connected".into());
        }
        Ok(guard.equipped.get(&user).cloned().into_iter().collect())
    }

    fn download_avatar(&self, hash: &str) -> Result<Vec<u8>, String> {
        let guard = state().lock().map_err(|_| "network poisoned".to_string())?;
        if !guard.connected && guard.mode != BackendMode::Mock {
            return Err("not connected".into());
        }
        if let Some(bytes) = guard.mock_store.get(hash) {
            return Ok(bytes.clone());
        }
        drop(guard);
        read_cache(hash)
    }

    fn upload_avatar(&self, bytes: &[u8]) -> Result<String, String> {
        let hash = content_hash(bytes);
        {
            let mut guard = state().lock().map_err(|_| "network poisoned".to_string())?;
            if !guard.connected {
                return Err("not connected".into());
            }
            if guard.mode == BackendMode::Live {
                
                
                return Err("live upload requires JVM session bridge".into());
            }
            guard.mock_store.insert(hash.clone(), bytes.to_vec());
        }
        write_cache(&hash, bytes)?;
        Ok(hash)
    }

    fn send_ping(&self, message: &PingMessage) -> Result<(), String> {
        let guard = state().lock().map_err(|_| "network poisoned".to_string())?;
        if !guard.connected {
            return Err("not connected".into());
        }
        if message.payload.len() > 1024 {
            return Err("ping payload too large".into());
        }
        Ok(())
    }
}

fn backend() -> ActiveBackend {
    ActiveBackend
}

pub fn discover_equipped(user: Uuid) -> Result<Vec<EquippedAvatar>, String> {
    let result = backend().discover_equipped(user);
    set_last_status_result(
        &result,
        |items| format!("discovered {} equipped avatars", items.len()),
        |err| err.clone(),
    );
    result
}

pub fn download_avatar(hash: &str) -> Result<Vec<u8>, String> {
    let result = backend().download_avatar(hash);
    set_last_status_result(
        &result,
        |bytes| format!("downloaded {} bytes", bytes.len()),
        |err| err.clone(),
    );
    result
}

pub fn upload_avatar(bytes: &[u8]) -> Result<String, String> {
    let result = backend().upload_avatar(bytes);
    set_last_status_result(
        &result,
        |hash| format!("uploaded avatar {hash}"),
        |err| err.clone(),
    );
    result
}

pub fn send_ping(message: &PingMessage) -> Result<(), String> {
    let result = backend().send_ping(message);
    set_last_status_result(&result, |_| "ping sent".to_string(), |err| err.clone());
    result
}

pub fn equip_local(owner: Uuid, hash: &str, name: impl Into<String>) -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| "network poisoned".to_string())?;
    if !guard.connected {
        return Err("not connected".into());
    }
    if !guard.mock_store.contains_key(hash) && read_cache(hash).is_err() {
        return Err(format!("unknown avatar hash: {hash}"));
    }
    let entry = EquippedAvatar {
        owner,
        hash: hash.to_string(),
        name: name.into(),
        download_url: Some(format!("{}{}/{}", guard.endpoint, paths::AVATARS, hash)),
    };
    guard.equipped.insert(owner, entry);
    guard.last_status = format!("equipped {hash} for {owner}");
    Ok(())
}

pub fn register_remote(player: Uuid, hash: impl Into<String>, name: impl Into<String>) {
    if let Ok(mut guard) = state().lock() {
        let hash = hash.into();
        let cached = guard.mock_store.contains_key(&hash) || read_cache(&hash).is_ok();
        guard.remotes.insert(
            player,
            RemotePlayerAvatar {
                player,
                hash,
                name: name.into(),
                cached,
            },
        );
        guard.last_status = format!("registered remote {player}");
    }
}

pub fn list_remotes() -> Vec<RemotePlayerAvatar> {
    state()
        .lock()
        .map(|guard| guard.remotes.values().cloned().collect())
        .unwrap_or_default()
}


pub fn sync_remote(player: Uuid) -> Result<Vec<u8>, String> {
    let hash = state()
        .lock()
        .map_err(|_| "network poisoned".to_string())?
        .remotes
        .get(&player)
        .map(|remote| remote.hash.clone())
        .ok_or_else(|| format!("no remote avatar for {player}"))?;
    let bytes = download_avatar(&hash)?;
    if let Ok(mut guard) = state().lock() {
        if let Some(remote) = guard.remotes.get_mut(&player) {
            remote.cached = true;
        }
    }
    Ok(bytes)
}

pub fn api_url(path: &str) -> String {
    let base = endpoint();
    let path = path.trim_start_matches('/');
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

pub fn encode_ping(message: &PingMessage) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + message.payload.len());
    out.push(message.id);
    out.extend_from_slice(&message.payload);
    out
}

pub fn decode_ping(bytes: &[u8]) -> Result<PingMessage, String> {
    let Some((id, payload)) = bytes.split_first() else {
        return Err("empty ping payload".into());
    };
    Ok(PingMessage {
        id: *id,
        payload: payload.to_vec(),
    })
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    let mut h2: u64 = 0x84222325cbf29ce4;
    for b in bytes.iter().rev().take(64) {
        h2 ^= u64::from(*b);
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}{h2:016x}")
}

fn normalize_endpoint(endpoint: String) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return DEFAULT_ENDPOINT.to_string();
    }
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn cache_path(hash: &str) -> Result<PathBuf, String> {
    let dir = cache_dir().map_err(|err| err.to_string())?;
    let safe: String = hash
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(64)
        .collect();
    if safe.is_empty() {
        return Err("invalid hash".into());
    }
    Ok(dir.join(format!("{safe}.avatar")))
}

fn write_cache(hash: &str, bytes: &[u8]) -> Result<(), String> {
    let path = cache_path(hash)?;
    fs::write(path, bytes).map_err(|err| err.to_string())
}

fn read_cache(hash: &str) -> Result<Vec<u8>, String> {
    let path = cache_path(hash)?;
    fs::read(path).map_err(|err| err.to_string())
}

fn set_last_status_result<T, FOk, FErr>(result: &Result<T, String>, ok: FOk, err: FErr)
where
    FOk: FnOnce(&T) -> String,
    FErr: FnOnce(&String) -> String,
{
    if let Ok(mut guard) = state().lock() {
        guard.last_status = match result {
            Ok(value) => ok(value),
            Err(message) => err(message),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_gate() -> &'static Mutex<()> {
        static GATE: OnceLock<Mutex<()>> = OnceLock::new();
        GATE.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn ping_framing_roundtrip() {
        let message = PingMessage {
            id: PING_PACKET_ID,
            payload: b"hello figura".to_vec(),
        };
        let encoded = encode_ping(&message);
        let decoded = decode_ping(&encoded).expect("decode ping");
        assert_eq!(decoded, message);
    }

    #[test]
    fn stub_backend_not_connected() {
        let _guard = test_gate()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        disconnect();
        let err = discover_equipped(Uuid::nil()).unwrap_err();
        assert_eq!(err, "not connected");
    }

    #[test]
    fn mock_upload_download_and_equip() {
        let _guard = test_gate()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        disconnect();
        connect_mock().expect("connect mock");
        let bytes = b"figura-avatar-bytes".to_vec();
        let hash = upload_avatar(&bytes).expect("upload");
        let downloaded = download_avatar(&hash).expect("download");
        assert_eq!(downloaded, bytes);
        equip_local(Uuid::nil(), &hash, "test").expect("equip");
        let equipped = discover_equipped(Uuid::nil()).expect("discover");
        assert_eq!(equipped.len(), 1);
        assert_eq!(equipped[0].hash, hash);
        disconnect();
    }

    #[test]
    fn remote_sync_uses_cache() {
        let _guard = test_gate()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        disconnect();
        connect_mock().expect("connect mock");
        let hash = upload_avatar(b"remote-avatar").expect("upload");
        let player = Uuid::from_u128(42);
        register_remote(player, &hash, "Remote");
        let bytes = sync_remote(player).expect("sync");
        assert_eq!(bytes, b"remote-avatar");
        disconnect();
    }
}
