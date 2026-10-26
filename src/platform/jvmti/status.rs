use std::sync::{OnceLock, RwLock};

#[derive(Clone, Debug, Default)]
pub struct JvmtiStatus {
    pub installed: bool,
    pub events_enabled: bool,
    pub trampoline_defined: bool,
    pub transformed_classes: usize,
    pub detected_runtime: String,
    pub last_error: String,
}

fn state() -> &'static RwLock<JvmtiStatus> {
    static STATE: OnceLock<RwLock<JvmtiStatus>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(JvmtiStatus::default()))
}

pub fn snapshot() -> JvmtiStatus {
    state()
        .read()
        .map(|status| status.clone())
        .unwrap_or_default()
}

pub fn update(f: impl FnOnce(&mut JvmtiStatus)) {
    if let Ok(mut status) = state().write() {
        f(&mut status);
    }
}

pub fn set_error(error: impl Into<String>) {
    update(|status| status.last_error = error.into());
}

pub fn clear_error() {
    update(|status| status.last_error.clear());
}
