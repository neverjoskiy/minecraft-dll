use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
    utils::trajectories,
};

pub struct TrajectoriesModule {
    menu: Module,
}

impl TrajectoriesModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "Trajectories",
                "Shows flight paths of arrows, tridents and ender pearls",
                Category::Render,
            )
            .with_setting(Setting::slider(
                trajectories::MAX_TICKS_SETTING,
                60.0,
                10.0,
                200.0,
            ))
            .with_setting(Setting::checkbox(trajectories::SHOW_HIT_SETTING, true))
            .with_setting(Setting::color_picker(
                trajectories::COLOR_SETTING,
                [0.33, 0.85, 1.0, 1.0],
            ))
            .with_setting(Setting::color_picker(
                trajectories::HIT_COLOR_SETTING,
                [1.0, 0.35, 0.35, 1.0],
            )),
        }
    }
}

impl ClientModule for TrajectoriesModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }
}
