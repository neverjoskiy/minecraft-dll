use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default)]
pub struct WheelAction {
    pub title: String,
    pub icon: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct WheelPage {
    pub title: String,
    pub slots: Vec<Option<WheelAction>>,
}

impl Default for WheelPage {
    fn default() -> Self {
        Self {
            title: String::new(),
            slots: vec![None; 8],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ActionWheelState {
    pub open: bool,
    pub page_index: usize,
    pub selected_index: usize,
    pub pages: Vec<WheelPage>,
    pub status: String,
}

#[derive(Clone, Debug, Default)]
pub struct OverlayRenderSnapshot {
    pub visible: bool,
    pub page_title: String,
    pub selected_index: usize,
    pub slot_titles: Vec<String>,
}

#[derive(Default)]
struct WheelController {
    state: ActionWheelState,
}

fn controller() -> &'static Mutex<WheelController> {
    static STATE: OnceLock<Mutex<WheelController>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(WheelController::default()))
}

pub fn status() -> String {
    controller()
        .lock()
        .map(|guard| guard.state.status.clone())
        .unwrap_or_else(|_| "action wheel poisoned".to_string())
}

pub fn state_snapshot() -> ActionWheelState {
    controller()
        .lock()
        .map(|guard| guard.state.clone())
        .unwrap_or_default()
}

pub fn overlay_snapshot() -> OverlayRenderSnapshot {
    controller()
        .lock()
        .map(|guard| overlay_from_state(&guard.state))
        .unwrap_or_default()
}

pub fn open() {
    if let Ok(mut guard) = controller().lock() {
        guard.state.open = true;
        guard.state.status = "wheel opened".to_string();
    }
}

pub fn close() {
    if let Ok(mut guard) = controller().lock() {
        guard.state.open = false;
        guard.state.status = "wheel closed".to_string();
    }
}

pub fn toggle() {
    if let Ok(mut guard) = controller().lock() {
        guard.state.open = !guard.state.open;
        guard.state.status = if guard.state.open {
            "wheel opened".to_string()
        } else {
            "wheel closed".to_string()
        };
    }
}

pub fn scroll(delta: i32) {
    if let Ok(mut guard) = controller().lock() {
        let page = current_page_mut(&mut guard.state);
        if page.slots.is_empty() {
            return;
        }
        let len = page.slots.len() as i32;
        let next = (guard.state.selected_index as i32 + delta).rem_euclid(len);
        guard.state.selected_index = next as usize;
        guard.state.status = format!("selected slot {}", guard.state.selected_index);
    }
}

pub fn select_slot(index: usize) {
    if let Ok(mut guard) = controller().lock() {
        let page = current_page_mut(&mut guard.state);
        if page.slots.is_empty() {
            return;
        }
        guard.state.selected_index = index % page.slots.len();
        guard.state.status = format!("selected slot {}", guard.state.selected_index);
    }
}

pub fn click() -> Option<WheelAction> {
    let action = {
        let guard = controller().lock().ok()?;
        let page = current_page(&guard.state)?;
        page.slots
            .get(guard.state.selected_index)
            .and_then(|slot| slot.clone())
    }?;
    if let Ok(mut guard) = controller().lock() {
        guard.state.status = format!("clicked {}", action.title);
    }
    Some(action)
}

pub fn set_title(page_index: usize, title: impl Into<String>) {
    if let Ok(mut guard) = controller().lock() {
        if let Some(page) = guard.state.pages.get_mut(page_index) {
            page.title = title.into();
            guard.state.status = format!("set page {page_index} title");
        }
    }
}

pub fn add_page(title: impl Into<String>) -> usize {
    controller()
        .lock()
        .map(|mut guard| {
            guard.state.pages.push(WheelPage {
                title: title.into(),
                ..Default::default()
            });
            guard.state.pages.len() - 1
        })
        .unwrap_or(0)
}

pub fn add_action(page_index: usize, slot_index: usize, action: WheelAction) -> bool {
    controller()
        .lock()
        .map(|mut guard| {
            let Some(page) = guard.state.pages.get_mut(page_index) else {
                return false;
            };
            if slot_index >= page.slots.len() {
                page.slots.resize(slot_index + 1, None);
            }
            page.slots[slot_index] = Some(action);
            guard.state.status = format!("action added at {page_index}:{slot_index}");
            true
        })
        .unwrap_or(false)
}

pub fn ensure_default_page() {
    if let Ok(mut guard) = controller().lock() {
        if guard.state.pages.is_empty() {
            let mut page = WheelPage {
                title: "Actions".to_string(),
                ..Default::default()
            };
            page.slots[0] = Some(WheelAction {
                title: "Ping".to_string(),
                icon: None,
                enabled: true,
            });
            page.slots[1] = Some(WheelAction {
                title: "Reload".to_string(),
                icon: None,
                enabled: true,
            });
            guard.state.pages.push(page);
        }
    }
}

fn current_page(state: &ActionWheelState) -> Option<&WheelPage> {
    state.pages.get(state.page_index)
}

fn current_page_mut(state: &mut ActionWheelState) -> &mut WheelPage {
    if state.pages.is_empty() {
        state.pages.push(WheelPage::default());
    }
    let idx = state.page_index.min(state.pages.len() - 1);
    state.page_index = idx;
    &mut state.pages[idx]
}

fn overlay_from_state(state: &ActionWheelState) -> OverlayRenderSnapshot {
    let page = current_page(state).cloned().unwrap_or_default();
    OverlayRenderSnapshot {
        visible: state.open,
        page_title: page.title,
        selected_index: state.selected_index,
        slot_titles: page
            .slots
            .iter()
            .map(|slot| {
                slot.as_ref()
                    .map(|action| action.title.clone())
                    .unwrap_or_default()
            })
            .collect(),
    }
}
