use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{minecraft::MinecraftContext, settings},
};

const FIRE_SETTING: &str = "Fire";
const SCOREBOARD_SETTING: &str = "Scoreboard";
const HURT_SHAKE_SETTING: &str = "Hurt shake";
const RUN_SHAKE_SETTING: &str = "Run shake";
const SCREEN_SHAKE_SETTING: &str = "Screen shake";
const FLUID_OVERLAY_SETTING: &str = "Water/lava blur";
const WARDEN_DARKNESS_SETTING: &str = "Warden darkness";
const NAUSEA_SETTING: &str = "Nausea";
const ATTACK_INDICATOR_SETTING: &str = "Attack indicator";
const EFFECTS_SETTING: &str = "Effects";
const BOSS_BAR_SETTING: &str = "Boss bar";
const BUBBLES_SETTING: &str = "Bubbles";
const GLOW_SETTING: &str = "Glow";
const RAIN_SETTING: &str = "Rain";
const VIGNETTE_SETTING: &str = "Vignette";

pub struct NoOverlayModule {
    menu: Module,
}

impl NoOverlayModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "NoOverlay",
                "Disables selected vanilla screen overlays and visual effects",
                Category::Render,
            )
            .with_setting(Setting::checkbox(FIRE_SETTING, true))
            .with_setting(Setting::checkbox(SCOREBOARD_SETTING, true))
            .with_setting(Setting::checkbox(HURT_SHAKE_SETTING, true))
            .with_setting(Setting::checkbox(RUN_SHAKE_SETTING, true))
            .with_setting(Setting::checkbox(SCREEN_SHAKE_SETTING, true))
            .with_setting(Setting::checkbox(FLUID_OVERLAY_SETTING, true))
            .with_setting(Setting::checkbox(WARDEN_DARKNESS_SETTING, true))
            .with_setting(Setting::checkbox(NAUSEA_SETTING, true))
            .with_setting(Setting::checkbox(ATTACK_INDICATOR_SETTING, true))
            .with_setting(Setting::checkbox(EFFECTS_SETTING, true))
            .with_setting(Setting::checkbox(BOSS_BAR_SETTING, true))
            .with_setting(Setting::checkbox(BUBBLES_SETTING, true))
            .with_setting(Setting::checkbox(GLOW_SETTING, false))
            .with_setting(Setting::checkbox(RAIN_SETTING, false))
            .with_setting(Setting::checkbox(VIGNETTE_SETTING, false)),
        }
    }
}

impl ClientModule for NoOverlayModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_tick(&mut self) {
        let config = NoOverlayConfig::from_module(&self.menu);
        let _ = jni_bridge::with_env(|env| {
            let result = tick_no_overlay(env, &config);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}

struct NoOverlayConfig {
    fire: bool,
    scoreboard: bool,
    hurt_shake: bool,
    run_shake: bool,
    warden_darkness: bool,
    nausea: bool,
    attack_indicator: bool,
    effects: bool,
    boss_bar: bool,
    bubbles: bool,
    glow: bool,
    rain: bool,
    vignette: bool,
}

impl NoOverlayConfig {
    fn from_module(module: &Module) -> Self {
        Self {
            fire: settings::checkbox(module, FIRE_SETTING, true),
            scoreboard: settings::checkbox(module, SCOREBOARD_SETTING, true),
            hurt_shake: settings::checkbox(module, HURT_SHAKE_SETTING, true),
            run_shake: settings::checkbox(module, RUN_SHAKE_SETTING, true),
            warden_darkness: settings::checkbox(module, WARDEN_DARKNESS_SETTING, true),
            nausea: settings::checkbox(module, NAUSEA_SETTING, true),
            attack_indicator: settings::checkbox(module, ATTACK_INDICATOR_SETTING, true),
            effects: settings::checkbox(module, EFFECTS_SETTING, true),
            boss_bar: settings::checkbox(module, BOSS_BAR_SETTING, true),
            bubbles: settings::checkbox(module, BUBBLES_SETTING, true),
            glow: settings::checkbox(module, GLOW_SETTING, false),
            rain: settings::checkbox(module, RAIN_SETTING, false),
            vignette: settings::checkbox(module, VIGNETTE_SETTING, false),
        }
    }
}

fn tick_no_overlay(env: &mut JNIEnv<'_>, config: &NoOverlayConfig) -> JniResult<()> {
    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };

    if config.fire {
        clear_player_fire(&mut ctx)?;
    }

    if config.rain {
        clear_weather(&mut ctx)?;
    }

    if config.hurt_shake {
        clear_hurt_shake(&mut ctx)?;
    }

    if config.run_shake {
        clear_run_shake(&mut ctx)?;
    }

    if config.warden_darkness {
        
        remove_player_effect(&mut ctx, "DARKNESS")?;
    }

    if config.nausea {
        
        remove_player_effect(&mut ctx, "CONFUSION")?;
    }

    if config.attack_indicator {
        clear_attack_indicator(&mut ctx)?;
    }

    if config.effects {
        remove_all_player_effects(&mut ctx)?;
    }

    if config.boss_bar {
        clear_boss_bar(&mut ctx)?;
    }

    if config.bubbles {
        clear_bubbles(&mut ctx)?;
    }

    if config.glow {
        clear_glow(&mut ctx)?;
    }

    if config.vignette {
        clear_vignette(&mut ctx)?;
    }

    
    

    Ok(())
}

fn clear_player_fire(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .set_field(&ctx.player, "remainingFireTicks", "I", JValue::Int(0))?;
    Ok(())
}

fn clear_weather(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .call_method(&ctx.level, "setRainLevel", "(F)V", &[JValue::Float(0.0)])?;
    ctx.env
        .call_method(&ctx.level, "setThunderLevel", "(F)V", &[JValue::Float(0.0)])?;
    Ok(())
}

fn clear_hurt_shake(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .set_field(&ctx.player, "hurtTime", "I", JValue::Int(0))?;
    Ok(())
}

fn clear_run_shake(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .set_field(&ctx.player, "walkDist", "F", JValue::Float(0.0))?;
    ctx.env
        .set_field(&ctx.player, "walkDistO", "F", JValue::Float(0.0))?;
    Ok(())
}

fn clear_screen_shake(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let _ = ctx
        .env
        .set_field(&ctx.player, "hurtTime", "I", JValue::Int(0));
    let _ = ctx
        .env
        .set_field(&ctx.player, "hurtDuration", "I", JValue::Int(0));
    let _ = ctx
        .env
        .set_field(&ctx.player, "bob", "F", JValue::Float(0.0));
    let _ = ctx
        .env
        .set_field(&ctx.player, "oBob", "F", JValue::Float(0.0));
    Ok(())
}

fn clear_fluid_overlay(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let _ = ctx.env.set_field(
        &ctx.player,
        "wasUnderwater",
        "Z",
        JValue::Bool(false.into()),
    );
    clear_bubbles(ctx)?;
    Ok(())
}

fn clear_bubbles(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .call_method(&ctx.player, "setAirSupply", "(I)V", &[JValue::Int(300)])?;
    Ok(())
}

fn clear_glow(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env.call_method(
        &ctx.player,
        "setGlowing",
        "(Z)V",
        &[JValue::Bool(false.into())],
    )?;
    Ok(())
}




fn clear_attack_indicator(_ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    Ok(())
}

fn clear_boss_bar(_ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    Ok(())
}

fn clear_vignette(_ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    Ok(())
}

fn remove_player_effect(ctx: &mut MinecraftContext<'_, '_>, effect_name: &str) -> JniResult<()> {
    let effect = mob_effect_holder(ctx.env, effect_name)?;
    let has_effect = ctx
        .env
        .call_method(
            &ctx.player,
            "hasEffect",
            "(Lnet/minecraft/core/Holder;)Z",
            &[JValue::Object(&effect)],
        )?
        .z()?;

    if has_effect {
        ctx.env.call_method(
            &ctx.player,
            "removeEffect",
            "(Lnet/minecraft/core/Holder;)Z",
            &[JValue::Object(&effect)],
        )?;
    }

    let _ = ctx.env.delete_local_ref(effect);
    Ok(())
}

fn remove_all_player_effects(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let effects = ctx
        .env
        .call_method(
            &ctx.player,
            "getActiveEffects",
            "()Ljava/util/Collection;",
            &[],
        )?
        .l()?;

    let iter = ctx
        .env
        .call_method(&effects, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    loop {
        let has_next = ctx.env.call_method(&iter, "hasNext", "()Z", &[])?.z()?;
        if !has_next {
            break;
        }
        let instance = ctx
            .env
            .call_method(&iter, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        let holder = ctx
            .env
            .call_method(&instance, "getEffect", "()Lnet/minecraft/core/Holder;", &[])?
            .l()?;

        let _ = ctx.env.call_method(
            &ctx.player,
            "removeEffect",
            "(Lnet/minecraft/core/Holder;)Z",
            &[JValue::Object(&holder)],
        );

        let _ = ctx.env.delete_local_ref(holder);
        let _ = ctx.env.delete_local_ref(instance);
    }

    let _ = ctx.env.delete_local_ref(iter);
    let _ = ctx.env.delete_local_ref(effects);
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
