use std::sync::{Mutex, OnceLock};

use jni::objects::JValue;

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge::{self, load_class},
    module_system::ClientModule,
    utils::minecraft::MinecraftContext,
};

const PRESET_SETTING: &str = "Time preset";

const PRESET_MORNING: &str = "Morning";
const PRESET_NIGHT: &str = "Night";
const PRESET_EVENING: &str = "Evening";
const PRESET_SUNSET: &str = "Sunset";

pub struct ChangeTimeModule {
    menu: Module,
}

impl ChangeTimeModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "ChangeTime",
                "Locks the in-game time of day to a chosen preset",
                Category::Render,
            )
            .with_setting(Setting::choice(
                PRESET_SETTING,
                [PRESET_MORNING, PRESET_NIGHT, PRESET_EVENING, PRESET_SUNSET],
                0,
            )),
        }
    }
}

impl ClientModule for ChangeTimeModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_tick(&mut self) {
        self.apply();
    }

    fn on_disable(&mut self) {
        if let Ok(mut slot) = last_ticks_slot().lock() {
            *slot = None;
        }
    }

    
    
}

impl ChangeTimeModule {
    fn apply(&self) {
        let _ = jni_bridge::with_env(|env| {
            let result = self.apply_env(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }

    fn apply_env(&self, env: &mut jni::JNIEnv<'_>) -> jni::errors::Result<()> {
        let preset = crate::utils::settings::choice(&self.menu, PRESET_SETTING, PRESET_MORNING);
        let ticks = match preset.as_str() {
            PRESET_NIGHT => 18000,
            PRESET_EVENING => 12000,
            PRESET_SUNSET => 13000,
            _ => 1000,
        };
        
        
        if !should_apply_ticks(ticks) {
            return Ok(());
        }
        let result = set_client_time(env, ticks);
        if result.is_ok() {
            mark_applied_ticks(ticks);
        }
        result
    }
}

fn set_client_time(env: &mut jni::JNIEnv<'_>, total_ticks: i64) -> jni::errors::Result<()> {
    let Some(ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };

    let clock_manager = ctx
        .env
        .call_method(
            &ctx.level,
            "clockManager",
            "()Lnet/minecraft/client/ClientClockManager;",
            &[],
        )?
        .l()?;
    if clock_manager.is_null() {
        return Ok(());
    }

    let clocks = ctx
        .env
        .get_field(&clock_manager, "clocks", "Ljava/util/Map;")?
        .l()?;
    if clocks.is_null() {
        return Ok(());
    }

    let overworld_key = {
        let world_clocks_class = load_class(ctx.env, "net.minecraft.world.clock.WorldClocks")?;
        let key = ctx
            .env
            .get_static_field(
                &world_clocks_class,
                "OVERWORLD",
                "Lnet/minecraft/resources/ResourceKey;",
            )?
            .l()?;
        let _ = ctx.env.delete_local_ref(world_clocks_class);
        key
    };

    let overworld_holder = {
        let registry_access = ctx
            .env
            .call_method(
                &ctx.level,
                "registryAccess",
                "()Lnet/minecraft/core/RegistryAccess;",
                &[],
            )?
            .l()?;
        let optional = ctx
            .env
            .call_method(
                &registry_access,
                "get",
                "(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;",
                &[JValue::Object(&overworld_key)],
            )?
            .l()?;
        let _ = ctx.env.delete_local_ref(registry_access);
        let holder = ctx
            .env
            .call_method(&optional, "get", "()Ljava/lang/Object;", &[])?
            .l()?;
        let _ = ctx.env.delete_local_ref(optional);
        holder
    };

    let clock_instance = ctx
        .env
        .call_method(
            &clocks,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[JValue::Object(&overworld_holder)],
        )?
        .l()?;

    if !clock_instance.is_null() {
        ctx.env.set_field(
            &clock_instance,
            "totalTicks",
            "J",
            JValue::Long(total_ticks),
        )?;
        ctx.env
            .set_field(&clock_instance, "partialTick", "F", JValue::Float(0.0))?;
        ctx.env
            .set_field(&clock_instance, "rate", "F", JValue::Float(0.0))?;
    }

    let env_attrs = ctx
        .env
        .call_method(
            &ctx.level,
            "environmentAttributes",
            "()Lnet/minecraft/world/attribute/EnvironmentAttributeSystem;",
            &[],
        )?
        .l()?;
    ctx.env
        .call_method(&env_attrs, "invalidateTickCache", "()V", &[])?;

    let _ = ctx.env.delete_local_ref(env_attrs);
    let _ = ctx.env.delete_local_ref(clock_instance);
    let _ = ctx.env.delete_local_ref(overworld_holder);
    let _ = ctx.env.delete_local_ref(overworld_key);
    let _ = ctx.env.delete_local_ref(clocks);
    let _ = ctx.env.delete_local_ref(clock_manager);

    
    
    
    let _ = ctx
        .env
        .call_method(
            &ctx.level,
            "setDayTime",
            "(J)V",
            &[JValue::Long(total_ticks % 24000)],
        );
    jni_bridge::clear_pending_exception(ctx.env);

    Ok(())
}

fn last_ticks_slot() -> &'static Mutex<Option<i64>> {
    static SLOT: OnceLock<Mutex<Option<i64>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn should_apply_ticks(ticks: i64) -> bool {
    last_ticks_slot()
        .lock()
        .ok()
        .map(|slot| *slot != Some(ticks))
        .unwrap_or(true)
}

fn mark_applied_ticks(ticks: i64) {
    if let Ok(mut slot) = last_ticks_slot().lock() {
        *slot = Some(ticks);
    }
}
