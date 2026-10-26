use std::sync::{Mutex, OnceLock};

use crate::{
    hooks,
    interface::{BindMode, BindTargetState, Module, Setting},
    utils::{hud_data::HudWorldData, name_tags::NameTagsConfig, potions::PotionEntry, settings},
};

#[derive(Clone, Debug)]
pub struct ActiveBindEntry {
    pub module_name: String,
    pub key_name: String,
}

#[derive(Clone, Debug, Default)]
pub struct HudSnapshot {
    pub enabled: bool,
    pub show_active_keybinds: bool,
    pub show_watermark: bool,
    pub show_potions: bool,
    pub show_target: bool,
    pub show_armor: bool,
    pub show_staff: bool,
    pub sync_theme: bool,
    pub active_binds: Vec<ActiveBindEntry>,
    pub potions: Vec<PotionEntry>,
    pub world: HudWorldData,
}

pub trait ClientModule: Send {
    fn menu_module(&self) -> &Module;
    fn menu_module_mut(&mut self) -> &mut Module;

    fn register(&mut self) {}
    fn on_enable(&mut self) {}
    fn on_disable(&mut self) {}
    fn on_frame(&mut self) {}
    fn on_tick(&mut self) {}
    fn on_action_bind(&mut self, _name: &str) {}
    fn load_config(&mut self) {}
    fn save_config(&self) {}
}

pub struct ModuleRegistry {
    modules: Vec<Box<dyn ClientModule>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    pub fn register(&mut self, mut module: Box<dyn ClientModule>) {
        module.register();
        self.modules.push(module);
    }

    pub fn tick_modules(&mut self) {
        for module in &mut self.modules {
            if module.menu_module().enabled {
                module.on_tick();
            }
        }
    }

    pub fn frame_modules(&mut self) {
        for module in &mut self.modules {
            if module.menu_module().enabled {
                module.on_frame();
            }
        }
    }

    pub fn process_binds(&mut self, pressed_keys: &[u32]) {
        for module in &mut self.modules {
            let Some(bind) = module.menu_module().bind.clone() else {
                continue;
            };
            if bind.key_code == 0 {
                continue;
            }

            match bind.mode {
                BindMode::Toggle => {
                    if pressed_keys.contains(&bind.key_code) {
                        let enabled = !module.menu_module().enabled;
                        set_module_enabled(module.as_mut(), enabled);
                    }
                }
                BindMode::Hold => {
                    let enabled = if hooks::is_key_down(bind.key_code) {
                        bind.target_state == BindTargetState::On
                    } else {
                        bind.target_state == BindTargetState::Off
                    };
                    set_module_enabled(module.as_mut(), enabled);
                }
            }
        }
    }

    pub fn hud_snapshot(&self) -> HudSnapshot {
        let Some(hud_module) = self
            .modules
            .iter()
            .map(|module| module.menu_module())
            .find(|module| module.name == "HUD")
        else {
            return HudSnapshot::default();
        };

        if !hud_module.enabled {
            return HudSnapshot::default();
        }

        HudSnapshot {
            enabled: true,
            show_active_keybinds: settings::checkbox(hud_module, "Active keybinds", true),
            show_watermark: settings::checkbox(hud_module, "Watermark", true),
            show_potions: settings::checkbox(hud_module, "Show potions", true),
            show_target: settings::checkbox(hud_module, "Target HUD", true),
            show_armor: settings::checkbox(hud_module, "Armor", true),
            show_staff: settings::checkbox(hud_module, "Staff list", true),
            sync_theme: settings::checkbox(hud_module, "Theme sync", true),
            active_binds: self.active_bind_entries(),
            potions: Vec::new(),
            world: Default::default(),
        }
    }

    pub fn name_tags_config(&self) -> Option<NameTagsConfig> {
        let module = self
            .modules
            .iter()
            .map(|module| module.menu_module())
            .find(|module| module.name == "NameTags")?;

        module.enabled.then(|| NameTagsConfig::from_module(module))
    }

    fn active_bind_entries(&self) -> Vec<ActiveBindEntry> {
        self.modules
            .iter()
            .map(|module| module.menu_module())
            .filter(|module| module.enabled)
            .filter_map(|module| {
                let bind = module.bind.as_ref()?;
                if bind.key_code == 0 {
                    return None;
                }

                Some(ActiveBindEntry {
                    module_name: module.name.clone(),
                    key_name: bind.key_name.clone(),
                })
            })
            .collect()
    }

    pub fn modules(&self) -> &[Box<dyn ClientModule>] {
        &self.modules
    }

    pub fn load_config(&mut self) {
        crate::config::load_active(self);
    }

    pub fn save_config(&self) {
        crate::config::save_active(self);
    }

    pub fn process_action_binds(&mut self, pressed_keys: &[u32]) {
        if pressed_keys.is_empty() {
            return;
        }

        for module in &mut self.modules {
            if !module.menu_module().enabled {
                continue;
            }

            let action_names: Vec<String> = module
                .menu_module()
                .settings
                .iter()
                .filter_map(|setting| match setting {
                    Setting::ActionBind(setting)
                        if setting.bind.key_code != 0
                            && pressed_keys.contains(&setting.bind.key_code) =>
                    {
                        Some(setting.name.clone())
                    }
                    _ => None,
                })
                .collect();

            for name in action_names {
                module.on_action_bind(&name);
            }
        }
    }

    pub fn disable_all(&mut self) {
        for module in &mut self.modules {
            set_module_enabled(module.as_mut(), false);
        }
    }

    pub fn modules_mut(&mut self) -> &mut [Box<dyn ClientModule>] {
        &mut self.modules
    }
}

pub fn set_module_enabled(module: &mut dyn ClientModule, enabled: bool) {
    let was_enabled = module.menu_module().enabled;
    if was_enabled == enabled {
        return;
    }

    module.menu_module_mut().enabled = enabled;
    if enabled {
        module.on_enable();
    } else {
        module.on_disable();
    }
}

static REGISTRY: OnceLock<Mutex<ModuleRegistry>> = OnceLock::new();

pub fn set_registry(registry: ModuleRegistry) -> Result<(), Mutex<ModuleRegistry>> {
    REGISTRY.set(Mutex::new(registry))
}

pub fn with_registry<R>(f: impl FnOnce(&mut ModuleRegistry) -> R) -> Option<R> {
    let registry = REGISTRY.get()?;
    let mut registry = registry.lock().ok()?;
    Some(f(&mut registry))
}
