use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
    sys::JNI_FALSE,
};

use crate::{
    interface::{Category, Module},
    jni_bridge,
    module_system::ClientModule,
    utils::minecraft::MinecraftContext,
};

const NIGHT_VISION_DURATION_TICKS: i32 = 400;

pub struct FullBrightModule {
    menu: Module,
}

impl FullBrightModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "FullBright",
                "Applies night vision so dark areas stay fully visible",
                Category::Render,
            ),
        }
    }
}

impl ClientModule for FullBrightModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_tick(&mut self) {
        let _ = jni_bridge::with_env(|env| {
            let result = apply_full_bright(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }

    fn on_disable(&mut self) {
        let _ = jni_bridge::with_env(|env| {
            let result = remove_full_bright(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}

fn apply_full_bright(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let Some(ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };

    let effect_instance_class =
        jni_bridge::load_class(ctx.env, "net.minecraft.world.effect.MobEffectInstance")?;
    let effect = mob_effect_holder(ctx.env, "NIGHT_VISION")?;
    let instance = ctx.env.new_object(
        effect_instance_class,
        "(Lnet/minecraft/core/Holder;IIZZZ)V",
        &[
            JValue::Object(&effect),
            JValue::Int(NIGHT_VISION_DURATION_TICKS),
            JValue::Int(0),
            JValue::Bool(JNI_FALSE),
            JValue::Bool(JNI_FALSE),
            JValue::Bool(JNI_FALSE),
        ],
    )?;

    ctx.env.call_method(
        &ctx.player,
        "addEffect",
        "(Lnet/minecraft/world/effect/MobEffectInstance;)Z",
        &[JValue::Object(&instance)],
    )?;

    let _ = ctx.env.delete_local_ref(instance);
    let _ = ctx.env.delete_local_ref(effect);
    Ok(())
}

fn remove_full_bright(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let Some(ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };

    let effect = mob_effect_holder(ctx.env, "NIGHT_VISION")?;
    ctx.env.call_method(
        &ctx.player,
        "removeEffect",
        "(Lnet/minecraft/core/Holder;)Z",
        &[JValue::Object(&effect)],
    )?;

    let _ = ctx.env.delete_local_ref(effect);
    Ok(())
}

fn mob_effect_holder<'local>(
    env: &mut JNIEnv<'local>,
    effect_name: &str,
) -> JniResult<JObject<'local>> {
    let effects = jni_bridge::load_class(env, "net.minecraft.world.effect.MobEffects")?;
    env.get_static_field(effects, effect_name, "Lnet/minecraft/core/Holder;")?
        .l()
}
