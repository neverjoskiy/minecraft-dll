use crate::{
    interface::{Category, Module},
    jni_bridge,
    module_system::ClientModule,
    utils::minecraft::MinecraftContext,
};

pub struct NoJumpDelayModule {
    menu: Module,
}

impl NoJumpDelayModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "NoJumpDelay",
                "Removes the delay between jumps",
                Category::Movement,
            ),
        }
    }
}

impl ClientModule for NoJumpDelayModule {
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

        let _ = jni_bridge::with_env(|env| {
            let Some(ctx) = MinecraftContext::new(env)? else {
                return Ok(());
            };

            let _ = ctx.env.set_field(
                &ctx.player,
                "noJumpDelay",
                "I",
                jni::objects::JValue::Int(0),
            );
            let _ = ctx.env.set_field(
                &ctx.player,
                "jumpTriggerTime",
                "I",
                jni::objects::JValue::Int(0),
            );

            Ok(())
        });
    }
}
