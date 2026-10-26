use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
    utils::{settings, world_markers},
};

pub const CHINA_HAT_COLOR_SETTING: &str = "Color";
pub const CHINA_HAT_RADIUS_SETTING: &str = "Radius";

pub struct ChinaHatModule {
    menu: Module,
}

impl ChinaHatModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "ChinaHat",
                "Draws a conic hat above the local player",
                Category::Render,
            )
            .with_setting(Setting::slider(CHINA_HAT_RADIUS_SETTING, 1.0, 0.5, 1.8))
            .with_setting(Setting::color_picker(
                CHINA_HAT_COLOR_SETTING,
                [0.65, 0.52, 1.0, 0.72],
            )),
        }
    }
}

impl ClientModule for ChinaHatModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    
    
    
    fn on_frame(&mut self) {
        let radius = settings::slider(&self.menu, CHINA_HAT_RADIUS_SETTING, 1.0).clamp(0.5, 1.8);
        let color =
            settings::color_picker(&self.menu, CHINA_HAT_COLOR_SETTING, [0.65, 0.52, 1.0, 0.72]);
        world_markers::set_china_hat_config(Some(world_markers::ChinaHatConfig {
            radius,
            color,
        }));
    }

    fn on_disable(&mut self) {
        world_markers::clear_china_hat_config();
    }
}
