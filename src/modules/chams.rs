use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        player_chams::{self, clear_all as clear_chams},
        player_esp::{
            BOX_COLOR_SETTING, FRIEND_COLOR_SETTING, PlayerEspConfig, RANGE_SETTING,
            SHOW_FRIENDS_SETTING, SHOW_SELF_SETTING,
        },
    },
};

pub struct ChamsModule {
    menu: Module,
}

impl ChamsModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "Chams",
                "Highlights players through walls",
                Category::Render,
            )
            .with_setting(Setting::slider(RANGE_SETTING, 64.0, 8.0, 128.0))
            .with_setting(Setting::checkbox(SHOW_FRIENDS_SETTING, true))
            .with_setting(Setting::checkbox(SHOW_SELF_SETTING, false))
            .with_setting(Setting::color_picker(
                BOX_COLOR_SETTING,
                [0.65, 0.52, 1.0, 1.0],
            ))
            .with_setting(Setting::color_picker(
                FRIEND_COLOR_SETTING,
                [0.25, 0.85, 0.45, 1.0],
            )),
        }
    }
}

impl ClientModule for ChamsModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_frame(&mut self) {
        if !self.menu.enabled {
            return;
        }
        let mut config = PlayerEspConfig::from_module(&self.menu);
        config.show_chams = true;
        
        
        config.show_box = false;
        config.show_name = false;
        config.show_health = false;
        config.show_hand_item = false;

        
        crate::utils::player_esp::set_world_config(Some(config.clone()));

        let _ = jni_bridge::with_env(|env| {
            let result = player_chams::apply_frame(env, &config);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }

    fn on_disable(&mut self) {
        crate::utils::player_esp::set_world_config(None);
        let _ = jni_bridge::with_env(|env| {
            let result = clear_chams(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}
