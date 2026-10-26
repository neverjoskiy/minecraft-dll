use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
    sys::{JNI_FALSE, JNI_TRUE},
};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::sprint_control,
};

const IGNORE_HUNGER_SETTING: &str = "Ignore Hunger";
const MIN_FOOD_FOR_AUTOSPRINT: i32 = 6;

pub struct AutoSprintModule {
    menu: Module,
}

impl AutoSprintModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "AutoSprint",
                "Automatically starts sprinting when forward movement is possible",
                Category::Movement,
            )
            .with_setting(Setting::checkbox(IGNORE_HUNGER_SETTING, false)),
        }
    }

    fn ignore_hunger_enabled(&self) -> bool {
        self.menu
            .settings
            .iter()
            .find_map(|setting| match setting {
                Setting::CheckBox(setting) if setting.name == IGNORE_HUNGER_SETTING => {
                    Some(setting.value)
                }
                _ => None,
            })
            .unwrap_or(false)
    }
}

impl ClientModule for AutoSprintModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_enable(&mut self) {
        sprint_control::set_auto_sprint_enabled(true);
    }

    fn on_disable(&mut self) {
        sprint_control::set_auto_sprint_enabled(false);
    }

    fn on_tick(&mut self) {
        sprint_control::set_auto_sprint_enabled(true);
        if sprint_control::triggerbot_suppressed() {
            let _ = jni_bridge::with_env(|env| {
                let result = stop_sprint_for_triggerbot(env);
                if result.is_err() {
                    jni_bridge::clear_pending_exception(env);
                }
                result
            });
            return;
        }

        let ignore_hunger = self.ignore_hunger_enabled();
        let _ = jni_bridge::with_env(|env| auto_sprint(env, ignore_hunger));
    }
}

fn stop_sprint_for_triggerbot(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let minecraft_class = jni_bridge::load_class(env, "net.minecraft.client.Minecraft")?;
    let minecraft = env
        .call_static_method(
            minecraft_class,
            "getInstance",
            "()Lnet/minecraft/client/Minecraft;",
            &[],
        )?
        .l()?;
    if minecraft.is_null() {
        return Ok(());
    }

    let player = env
        .get_field(
            &minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )?
        .l()?;
    let options = env
        .get_field(&minecraft, "options", "Lnet/minecraft/client/Options;")?
        .l()?;
    if player.is_null() || options.is_null() {
        return Ok(());
    }

    let sprint_key = env
        .get_field(&options, "keySprint", "Lnet/minecraft/client/KeyMapping;")?
        .l()?;
    if !sprint_key.is_null() {
        env.call_method(&sprint_key, "setDown", "(Z)V", &[JValue::Bool(JNI_FALSE)])?;
        let _ = env.delete_local_ref(sprint_key);
    }
    env.call_method(&player, "setSprinting", "(Z)V", &[JValue::Bool(JNI_FALSE)])?;

    Ok(())
}

fn auto_sprint(env: &mut JNIEnv<'_>, ignore_hunger: bool) -> JniResult<()> {
    let minecraft_class = jni_bridge::load_class(env, "net.minecraft.client.Minecraft")?;
    let minecraft = env
        .call_static_method(
            minecraft_class,
            "getInstance",
            "()Lnet/minecraft/client/Minecraft;",
            &[],
        )?
        .l()?;
    if minecraft.is_null() {
        return Ok(());
    }

    let player = env
        .get_field(
            &minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )?
        .l()?;
    if player.is_null() {
        return Ok(());
    }

    if !has_forward_impulse(env, &player)? {
        return Ok(());
    }

    if !ignore_hunger && food_level(env, &player)? < MIN_FOOD_FOR_AUTOSPRINT {
        return Ok(());
    }

    if can_start_sprinting(env, &player).unwrap_or(true) {
        env.call_method(&player, "setSprinting", "(Z)V", &[JValue::Bool(JNI_TRUE)])?;
    }

    Ok(())
}

fn has_forward_impulse(env: &mut JNIEnv<'_>, player: &JObject<'_>) -> JniResult<bool> {
    let input = env
        .get_field(player, "input", "Lnet/minecraft/client/player/ClientInput;")?
        .l()?;
    if input.is_null() {
        return Ok(false);
    }

    env.call_method(&input, "hasForwardImpulse", "()Z", &[])?
        .z()
}

fn food_level(env: &mut JNIEnv<'_>, player: &JObject<'_>) -> JniResult<i32> {
    let food_data = env
        .call_method(
            player,
            "getFoodData",
            "()Lnet/minecraft/world/food/FoodData;",
            &[],
        )?
        .l()?;
    if food_data.is_null() {
        return Ok(0);
    }

    env.call_method(&food_data, "getFoodLevel", "()I", &[])?.i()
}

fn can_start_sprinting(env: &mut JNIEnv<'_>, player: &JObject<'_>) -> JniResult<bool> {
    let result = env
        .call_method(player, "canStartSprinting", "()Z", &[])
        .and_then(|value| value.z());

    if result.is_err() {
        jni_bridge::clear_pending_exception(env);
    }

    result
}
