use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub process: Option<String>,
    pub pid: Option<u32>,
    pub dll: Option<String>,
    pub method: Option<String>,
}

fn config_path() -> PathBuf {
    std::env::temp_dir().join("injector").join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Config::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save(config: &Config) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(&path, json);
    }
}
