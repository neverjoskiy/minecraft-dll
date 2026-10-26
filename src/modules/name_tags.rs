use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
    utils::name_tags::{
        self, ALPHA_SETTING, FRIEND_COLOR_SETTING, HEALTH_HEARTS, HEALTH_MODE_SETTING,
        HEALTH_PERCENT, HEALTH_POINTS, RANGE_SETTING, SHOW_ARMOR_SETTING, SHOW_DONATE_SETTING,
        SHOW_FRIENDS_SETTING, SHOW_HAND_ITEMS_SETTING, SHOW_SELF_SETTING, SIZE_SETTING,
        STATIC_SIZE_SETTING,
    },
};

pub struct NameTagsModule {
    menu: Module,
}

impl NameTagsModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "NameTags",
                "Draws player names, health, and equipment above players",
                Category::Render,
            )
            .with_setting(Setting::choice(
                HEALTH_MODE_SETTING,
                [HEALTH_POINTS, HEALTH_HEARTS, HEALTH_PERCENT],
                0,
            ))
            .with_setting(Setting::slider(SIZE_SETTING, 0.35, 0.01, 1.0))
            .with_setting(Setting::slider(ALPHA_SETTING, 1.0, 0.0, 1.0))
            .with_setting(Setting::slider(RANGE_SETTING, 64.0, 8.0, 128.0))
            .with_setting(Setting::checkbox(SHOW_ARMOR_SETTING, true))
            .with_setting(Setting::checkbox(SHOW_HAND_ITEMS_SETTING, true))
            .with_setting(Setting::checkbox(SHOW_DONATE_SETTING, false))
            .with_setting(Setting::checkbox(STATIC_SIZE_SETTING, false))
            .with_setting(Setting::checkbox(SHOW_FRIENDS_SETTING, true))
            .with_setting(Setting::checkbox(SHOW_SELF_SETTING, false))
            .with_setting(Setting::color_picker(
                FRIEND_COLOR_SETTING,
                [0.25, 0.85, 0.45, 1.0],
            )),
        }
    }
}

impl ClientModule for NameTagsModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_enable(&mut self) {
        self.push_world_config();
    }

    fn on_disable(&mut self) {
        name_tags::world::set_world_config(None);
    }

    fn on_tick(&mut self) {
        if self.menu.enabled {
            self.push_world_config();
        }
    }

    fn on_frame(&mut self) {
        if self.menu.enabled {
            self.push_world_config();
        }
    }
}

impl NameTagsModule {
    fn push_world_config(&self) {
        let cfg = name_tags::NameTagsConfig::from_module(&self.menu);
        name_tags::world::set_world_config(Some(cfg));
    }
}
