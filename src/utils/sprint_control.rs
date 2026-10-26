use std::sync::atomic::{AtomicBool, Ordering};

static TRIGGERBOT_SUPPRESSING_SPRINT: AtomicBool = AtomicBool::new(false);
static AUTO_SPRINT_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_triggerbot_suppressed(suppressed: bool) {
    TRIGGERBOT_SUPPRESSING_SPRINT.store(suppressed, Ordering::Release);
}

pub fn triggerbot_suppressed() -> bool {
    TRIGGERBOT_SUPPRESSING_SPRINT.load(Ordering::Acquire)
}

pub fn set_auto_sprint_enabled(enabled: bool) {
    AUTO_SPRINT_ENABLED.store(enabled, Ordering::Release);
}

pub fn auto_sprint_enabled() -> bool {
    AUTO_SPRINT_ENABLED.load(Ordering::Acquire)
}
