use crate::interface::{Module, Setting};

pub fn checkbox(module: &Module, name: &str, default: bool) -> bool {
    module
        .settings
        .iter()
        .find_map(|setting| match setting {
            Setting::CheckBox(setting) if setting.name == name => Some(setting.value),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn slider(module: &Module, name: &str, default: f32) -> f32 {
    module
        .settings
        .iter()
        .find_map(|setting| match setting {
            Setting::Slider(setting) if setting.name == name => Some(setting.value),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn choice(module: &Module, name: &str, default: &str) -> String {
    module
        .settings
        .iter()
        .find_map(|setting| match setting {
            Setting::Choice(setting) if setting.name == name => {
                Some(setting.selected().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| default.to_string())
}

pub fn color_picker(module: &Module, name: &str, default: [f32; 4]) -> [f32; 4] {
    module
        .settings
        .iter()
        .find_map(|setting| match setting {
            Setting::ColorPicker(setting) if setting.name == name => Some(setting.value),
            _ => None,
        })
        .unwrap_or(default)
}
