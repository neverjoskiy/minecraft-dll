use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

use crate::{
    interface::{Bind, BindMode, BindTargetState, Module, Setting},
    module_system::{self, ClientModule, ModuleRegistry},
    utils::{friends, random},
};

const LOCATOR_NAME: &str = ".xz_cfg_root";
const DEFAULT_CONFIG: &str = "default";
const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredBind {
    key_name: String,
    key_code: u32,
    mode: String,
    target_state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StoredSetting {
    Checkbox { name: String, value: bool },
    Slider { name: String, value: f32 },
    ColorPicker { name: String, value: [f32; 4] },
    ActionBind { name: String, bind: StoredBind },
    Choice { name: String, selected_index: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredModule {
    name: String,
    enabled: bool,
    bind: Option<StoredBind>,
    settings: Vec<StoredSetting>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConfigFile {
    version: u32,
    theme: Option<String>,
    friends: Vec<String>,
    modules: Vec<StoredModule>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    figura_endpoint: Option<String>,
}

#[derive(Default)]
struct ConfigState {
    root_dir: Option<PathBuf>,
    active: String,
    last_status: String,
}

fn state() -> &'static Mutex<ConfigState> {
    static STATE: OnceLock<Mutex<ConfigState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(ConfigState {
            root_dir: None,
            active: DEFAULT_CONFIG.to_string(),
            last_status: String::new(),
        })
    })
}

fn set_status(message: impl Into<String>) {
    if let Ok(mut guard) = state().lock() {
        guard.last_status = message.into();
    }
}

pub fn status() -> String {
    state()
        .lock()
        .map(|guard| guard.last_status.clone())
        .unwrap_or_default()
}

pub fn active_name() -> String {
    state()
        .lock()
        .map(|guard| {
            if guard.active.is_empty() {
                DEFAULT_CONFIG.to_string()
            } else {
                guard.active.clone()
            }
        })
        .unwrap_or_else(|_| DEFAULT_CONFIG.to_string())
}

pub fn set_active_name(name: &str) {
    let sanitized = sanitize_name(name);
    if sanitized.is_empty() {
        return;
    }
    if let Ok(mut guard) = state().lock() {
        guard.active = sanitized;
    }
}


pub fn ensure_root_dir() -> io::Result<PathBuf> {
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(path) = guard.root_dir.clone() {
        if path.is_dir() {
            return Ok(path);
        }
    }

    let temp = std::env::temp_dir();
    let locator = temp.join(LOCATOR_NAME);

    if let Ok(name) = fs::read_to_string(&locator) {
        let name = name.trim();
        if !name.is_empty() {
            let path = temp.join(name);
            if path.is_dir() || fs::create_dir_all(&path).is_ok() {
                guard.root_dir = Some(path.clone());
                return Ok(path);
            }
        }
    }

    let folder_name = format!(
        "xz_{:016x}{:08x}",
        random::next_u64(),
        random::range_u64(u32::MAX as u64) as u32
    );
    let path = temp.join(&folder_name);
    fs::create_dir_all(&path)?;
    fs::write(&locator, &folder_name)?;
    guard.root_dir = Some(path.clone());
    Ok(path)
}

fn config_path(name: &str) -> io::Result<PathBuf> {
    let root = ensure_root_dir()?;
    Ok(root.join(format!("{}.json", sanitize_name(name))))
}

pub fn list_configs() -> Vec<String> {
    let Ok(root) = ensure_root_dir() else {
        return vec![DEFAULT_CONFIG.to_string()];
    };

    let mut names = match fs::read_dir(&root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| stem.to_string())
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };

    if !names.iter().any(|name| name == DEFAULT_CONFIG) {
        names.push(DEFAULT_CONFIG.to_string());
    }
    names.sort();
    names.dedup();
    names
}

pub fn save_named(registry: &ModuleRegistry, name: &str) -> Result<(), String> {
    let name = sanitize_name(name);
    if name.is_empty() {
        set_status("Пустое имя конфига");
        return Err("empty name".into());
    }

    let path = config_path(&name).map_err(|err| err.to_string())?;
    let file = ConfigFile {
        version: CONFIG_VERSION,
        theme: None,
        friends: friends::list(),
        modules: registry
            .modules()
            .iter()
            .map(|module| store_module(module.menu_module()))
            .collect(),
        figura_endpoint: figura_endpoint(),
    };

    let json = serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?;
    write_atomic(&path, json.as_bytes()).map_err(|err| err.to_string())?;

    set_active_name(&name);
    set_status(format!("Сохранено: {name}"));
    Ok(())
}

pub fn load_named(registry: &mut ModuleRegistry, name: &str) -> Result<(), String> {
    let name = sanitize_name(name);
    if name.is_empty() {
        set_status("Пустое имя конфига");
        return Err("empty name".into());
    }

    let path = config_path(&name).map_err(|err| err.to_string())?;
    if !path.is_file() {
        set_status(format!("Конфиг не найден: {name}"));
        return Err("missing".into());
    }

    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let file: ConfigFile = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    apply_config(registry, &file);
    set_active_name(&name);
    set_status(format!("Загружено: {name}"));
    Ok(())
}

pub fn delete_named(name: &str) -> Result<(), String> {
    let name = sanitize_name(name);
    if name.is_empty() || name == DEFAULT_CONFIG {
        set_status("Нельзя удалить default");
        return Err("protected".into());
    }

    let path = config_path(&name).map_err(|err| err.to_string())?;
    if path.is_file() {
        fs::remove_file(&path).map_err(|err| err.to_string())?;
    }
    if active_name() == name {
        set_active_name(DEFAULT_CONFIG);
    }
    set_status(format!("Удалено: {name}"));
    Ok(())
}

pub fn save_active(registry: &ModuleRegistry) {
    let name = active_name();
    let _ = save_named(registry, &name);
}

pub fn load_active(registry: &mut ModuleRegistry) {
    let name = active_name();
    let path = match config_path(&name) {
        Ok(path) => path,
        Err(_) => return,
    };
    if path.is_file() {
        let _ = load_named(registry, &name);
    } else {
        
        let _ = save_named(registry, DEFAULT_CONFIG);
        set_status("Создан default конфиг");
    }
}


pub fn figura_endpoint() -> Option<String> {
    let Ok(root) = ensure_root_dir() else {
        return None;
    };
    let path = root.join("figura_endpoint.txt");
    fs::read_to_string(path)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn set_figura_endpoint(endpoint: &str) -> io::Result<()> {
    let root = ensure_root_dir()?;
    let path = root.join("figura_endpoint.txt");
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    fs::write(path, trimmed)
}

fn apply_config(registry: &mut ModuleRegistry, file: &ConfigFile) {
    friends::replace_all(file.friends.clone());
    if let Some(endpoint) = file.figura_endpoint.as_deref() {
        let _ = set_figura_endpoint(endpoint);
    }

    for stored in &file.modules {
        let Some(module) = registry
            .modules_mut()
            .iter_mut()
            .find(|module| module.menu_module().name == stored.name)
        else {
            continue;
        };

        apply_module(module.as_mut(), stored);
    }
}

fn apply_module(module: &mut dyn ClientModule, stored: &StoredModule) {
    {
        let menu = module.menu_module_mut();
        menu.bind = stored.bind.as_ref().map(bind_from_stored);
        for setting in &mut menu.settings {
            let name = setting_name(setting);
            let Some(stored_setting) = stored
                .settings
                .iter()
                .find(|item| setting_stored_name(item) == name)
            else {
                continue;
            };
            apply_setting(setting, stored_setting);
        }
    }
    module_system::set_module_enabled(module, stored.enabled);
}

fn apply_setting(setting: &mut Setting, stored: &StoredSetting) {
    match (setting, stored) {
        (Setting::CheckBox(target), StoredSetting::Checkbox { value, .. }) => {
            target.value = *value;
        }
        (Setting::Slider(target), StoredSetting::Slider { value, .. }) => {
            target.value = (*value).clamp(target.min, target.max);
        }
        (Setting::ColorPicker(target), StoredSetting::ColorPicker { value, .. }) => {
            target.value = *value;
        }
        (Setting::ActionBind(target), StoredSetting::ActionBind { bind, .. }) => {
            target.bind = bind_from_stored(bind);
        }
        (Setting::Choice(target), StoredSetting::Choice { selected_index, .. }) => {
            if !target.options.is_empty() {
                target.selected_index = (*selected_index).min(target.options.len() - 1);
            }
        }
        _ => {}
    }
}

fn store_module(module: &Module) -> StoredModule {
    StoredModule {
        name: module.name.clone(),
        enabled: module.enabled,
        bind: module.bind.as_ref().map(store_bind),
        settings: module.settings.iter().filter_map(store_setting).collect(),
    }
}

fn store_bind(bind: &Bind) -> StoredBind {
    StoredBind {
        key_name: bind.key_name.clone(),
        key_code: bind.key_code,
        mode: bind.mode.as_str().to_string(),
        target_state: bind.target_state.as_str().to_string(),
    }
}

fn bind_from_stored(bind: &StoredBind) -> Bind {
    Bind {
        key_name: bind.key_name.clone(),
        key_code: bind.key_code,
        mode: match bind.mode.as_str() {
            "Hold" => BindMode::Hold,
            _ => BindMode::Toggle,
        },
        target_state: match bind.target_state.as_str() {
            "Off" => BindTargetState::Off,
            _ => BindTargetState::On,
        },
    }
}

fn store_setting(setting: &Setting) -> Option<StoredSetting> {
    Some(match setting {
        Setting::CheckBox(value) => StoredSetting::Checkbox {
            name: value.name.clone(),
            value: value.value,
        },
        Setting::Slider(value) => StoredSetting::Slider {
            name: value.name.clone(),
            value: value.value,
        },
        Setting::ColorPicker(value) => StoredSetting::ColorPicker {
            name: value.name.clone(),
            value: value.value,
        },
        Setting::ActionBind(value) => StoredSetting::ActionBind {
            name: value.name.clone(),
            bind: store_bind(&value.bind),
        },
        Setting::Choice(value) => StoredSetting::Choice {
            name: value.name.clone(),
            selected_index: value.selected_index,
        },
    })
}

fn setting_name(setting: &Setting) -> &str {
    match setting {
        Setting::CheckBox(value) => &value.name,
        Setting::Slider(value) => &value.name,
        Setting::ColorPicker(value) => &value.name,
        Setting::ActionBind(value) => &value.name,
        Setting::Choice(value) => &value.name,
    }
}

fn setting_stored_name(setting: &StoredSetting) -> &str {
    match setting {
        StoredSetting::Checkbox { name, .. }
        | StoredSetting::Slider { name, .. }
        | StoredSetting::ColorPicker { name, .. }
        | StoredSetting::ActionBind { name, .. }
        | StoredSetting::Choice { name, .. } => name,
    }
}

fn sanitize_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    out.chars().take(48).collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
