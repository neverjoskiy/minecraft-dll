use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeuroProgressPhase {
    Recording,
    Training,
}

impl Default for NeuroProgressPhase {
    fn default() -> Self {
        Self::Recording
    }
}

#[derive(Clone, Debug, Default)]
pub struct NeuroProgressOverlay {
    pub visible: bool,
    pub phase: NeuroProgressPhase,
    pub samples: u64,
    pub max_samples: u64,
    pub detail: String,
}

pub fn set_progress_overlay(state: NeuroProgressOverlay) {
    let slot = PROGRESS.get_or_init(|| Mutex::new(NeuroProgressOverlay::default()));
    if let Ok(mut guard) = slot.lock() {
        *guard = state;
    }
}

pub fn clear_progress_overlay() {
    set_progress_overlay(NeuroProgressOverlay::default());
}

pub fn progress_overlay() -> NeuroProgressOverlay {
    let slot = PROGRESS.get_or_init(|| Mutex::new(NeuroProgressOverlay::default()));
    slot.lock().map(|guard| guard.clone()).unwrap_or_default()
}

static PROGRESS: OnceLock<Mutex<NeuroProgressOverlay>> = OnceLock::new();
