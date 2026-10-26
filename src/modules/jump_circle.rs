use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
    utils::jump_circle,
};

pub struct JumpCircleModule {
    menu: Module,
}

impl JumpCircleModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "JumpCircle",
                "Мягкое светящееся кольцо на земле при прыжке",
                Category::Render,
            )
            .with_setting(Setting::slider(
                jump_circle::LIFETIME_SETTING,
                0.9,
                0.2,
                3.0,
            ))
            .with_setting(Setting::slider(jump_circle::SPEED_SETTING, 1.0, 0.1, 3.0))
            .with_setting(Setting::checkbox(jump_circle::THEME_SYNC_SETTING, true)),
        }
    }
}

impl ClientModule for JumpCircleModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    
    
    fn on_tick(&mut self) {
        jump_circle::tick(&self.menu);
    }

    fn on_disable(&mut self) {
        jump_circle::clear();
    }
}
