use jni::{JNIEnv, errors::Result as JniResult};

use crate::{
    interface::{Category, Module},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        friends,
        minecraft::{self, MinecraftContext},
    },
};

pub struct NoFriendDamageModule {
    menu: Module,
}

impl NoFriendDamageModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "NoFriendDamage",
                "Blocks attacks and Combat targets for players from the friends list",
                Category::Combat,
            ),
        }
    }

    fn tick_env(&mut self, env: &mut JNIEnv<'_>) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(());
        }

        if minecraft::crosshair_target_is_friend(&mut ctx)? {
            minecraft::clear_queued_attack_clicks(&mut ctx)?;
        }

        Ok(())
    }
}

impl ClientModule for NoFriendDamageModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_enable(&mut self) {
        friends::set_no_friend_damage_active(true);
    }

    fn on_disable(&mut self) {
        friends::set_no_friend_damage_active(false);
    }

    fn on_tick(&mut self) {
        friends::set_no_friend_damage_active(self.menu.enabled);
        let _ = jni_bridge::with_env(|env| {
            let result = self.tick_env(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}
