use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
};

pub struct HudModule {
    menu: Module,
}

impl HudModule {
    pub fn new() -> Self {
        let mut menu = Module::new("HUD", "Displays client overlay elements", Category::Render)
            .with_setting(Setting::checkbox("Active keybinds", true))
            .with_setting(Setting::checkbox("Watermark", true))
            .with_setting(Setting::checkbox("Show potions", true))
            .with_setting(Setting::checkbox("Target HUD", true))
            .with_setting(Setting::checkbox("Armor", true))
            .with_setting(Setting::checkbox("Staff list", true))
            .with_setting(Setting::checkbox("Theme sync", true));
        menu.enabled = true;

        Self { menu }
    }
}

impl ClientModule for HudModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }
}
