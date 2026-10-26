use std::sync::{Mutex, OnceLock};

use jni::{
    errors::Result as JniResult,
    objects::{JClass, JObject, JString, JValue},
};

use crate::{
    jni_bridge,
    utils::{
        friends,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        rotation::Angle,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AimPriority {
    Distance,
    Health,
}

#[derive(Clone, Copy, Debug)]
pub struct AimTargetConfig {
    pub distance: f64,
    pub fov_degrees: f64,
    pub aim_height: f64,
    pub priority: AimPriority,
    pub only_players: bool,
    pub hit_invisible: bool,
    pub only_armored: bool,
    pub ignore_naked: bool,
    pub wall_check: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct AimTarget {
    pub entity_id: i32,
    pub position: Vec3d,
    pub width: f64,
    pub height: f64,
    pub distance: f64,
    pub health: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct AimAssistOverlay {
    pub visible: bool,
    pub target_active: bool,
    pub target_entity_id: Option<i32>,
    pub aim_fov_degrees: f32,
    pub vanilla_fov_degrees: f32,
}

impl Default for AimAssistOverlay {
    fn default() -> Self {
        Self {
            visible: false,
            target_active: false,
            target_entity_id: None,
            aim_fov_degrees: 90.0,
            vanilla_fov_degrees: 90.0,
        }
    }
}

pub fn set_overlay_state(state: AimAssistOverlay) {
    let overlay = OVERLAY.get_or_init(|| Mutex::new(AimAssistOverlay::default()));
    if let Ok(mut overlay) = overlay.lock() {
        *overlay = state;
    }
}

pub fn clear_overlay_state() {
    set_overlay_state(AimAssistOverlay::default());
}

pub fn overlay_state() -> AimAssistOverlay {
    let overlay = OVERLAY.get_or_init(|| Mutex::new(AimAssistOverlay::default()));
    overlay.lock().map(|overlay| *overlay).unwrap_or_default()
}

pub fn find_best_target(
    ctx: &mut MinecraftContext<'_, '_>,
    config: &AimTargetConfig,
) -> JniResult<Option<AimTarget>> {
    let classes = AimEntityClasses {
        living: jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.LivingEntity")?,
        player: jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?,
    };
    let eye = player_eye_position(ctx)?;
    let view = player_view_vector(ctx)?;
    let max_angle = (config.fov_degrees * 0.5).clamp(0.0, 180.0);

    let iterable = ctx
        .env
        .call_method(
            &ctx.level,
            "entitiesForRendering",
            "()Ljava/lang/Iterable;",
            &[],
        )?
        .l()?;
    let iterator = ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let mut best: Option<(AimTarget, f64)> = None;
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;

        if let Some(target) =
            target_from_entity(ctx, &entity, &classes, config, eye, view, max_angle)?
        {
            let score = match config.priority {
                AimPriority::Distance => target.distance,
                AimPriority::Health => target.health,
            };
            if best
                .as_ref()
                .is_none_or(|(_, current_score)| score < *current_score)
            {
                best = Some((target, score));
            }
        }

        let _ = ctx.env.delete_local_ref(entity);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);
    Ok(best.map(|(target, _)| target))
}

pub fn player_rotation(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Angle> {
    Ok(Angle::new(
        ctx.env
            .call_method(&ctx.player, "getYRot", "()F", &[])?
            .f()? as f64,
        ctx.env
            .call_method(&ctx.player, "getXRot", "()F", &[])?
            .f()? as f64,
    ))
}

pub fn set_player_rotation(ctx: &mut MinecraftContext<'_, '_>, rotation: Angle) -> JniResult<()> {
    ctx.env.call_method(
        &ctx.player,
        "setYRot",
        "(F)V",
        &[JValue::Float(rotation.yaw as f32)],
    )?;
    ctx.env.call_method(
        &ctx.player,
        "setXRot",
        "(F)V",
        &[JValue::Float(rotation.pitch.clamp(-90.0, 90.0) as f32)],
    )?;
    Ok(())
}

pub fn player_eye_position(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    let vector = ctx
        .env
        .call_method(
            &ctx.player,
            "getEyePosition",
            "()Lnet/minecraft/world/phys/Vec3;",
            &[],
        )?
        .l()?;
    let result = vec3_from_java(ctx.env, &vector)?;
    let _ = ctx.env.delete_local_ref(vector);
    Ok(result)
}

pub fn player_velocity_y(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f64> {
    minecraft::player_velocity_y(ctx)
}

pub fn player_fall_distance(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f64> {
    ctx.env.get_field(&ctx.player, "fallDistance", "D")?.d()
}

pub fn player_on_ground(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    minecraft::player_on_ground(ctx)
}

pub fn screen_is_container(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    let Some(screen) = current_screen(ctx)? else {
        return Ok(false);
    };
    let container_screen = jni_bridge::load_class(
        ctx.env,
        "net.minecraft.client.gui.screens.inventory.AbstractContainerScreen",
    )?;
    let result = ctx.env.is_instance_of(&screen, &container_screen)?;
    let _ = ctx.env.delete_local_ref(screen);
    Ok(result)
}

pub fn screen_is_none(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    Ok(current_screen(ctx)?.is_none())
}

pub fn vanilla_fov(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f32> {
    let fov_option = ctx
        .env
        .call_method(
            &ctx.options,
            "fov",
            "()Lnet/minecraft/client/OptionInstance;",
            &[],
        )?
        .l()?;
    let value = ctx
        .env
        .call_method(&fov_option, "get", "()Ljava/lang/Object;", &[])?
        .l()?;
    let fov = java_number_as_f64(ctx, &value).unwrap_or(90.0) as f32;
    let _ = ctx.env.delete_local_ref(value);
    let _ = ctx.env.delete_local_ref(fov_option);
    Ok(fov.clamp(30.0, 170.0))
}

pub fn dimension_key(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<String> {
    let dimension = ctx
        .env
        .call_method(
            &ctx.level,
            "dimension",
            "()Lnet/minecraft/resources/ResourceKey;",
            &[],
        )?
        .l()?;
    let id = ctx
        .env
        .call_method(
            &dimension,
            "identifier",
            "()Lnet/minecraft/resources/Identifier;",
            &[],
        )?
        .l()?;
    let string = ctx
        .env
        .call_method(&id, "toString", "()Ljava/lang/String;", &[])?
        .l()?;
    let key = java_string(ctx, string)?;
    let _ = ctx.env.delete_local_ref(id);
    let _ = ctx.env.delete_local_ref(dimension);
    Ok(key)
}

struct AimEntityClasses<'local> {
    living: JClass<'local>,
    player: JClass<'local>,
}

fn target_from_entity(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    classes: &AimEntityClasses<'_>,
    config: &AimTargetConfig,
    eye: Vec3d,
    view: Vec3d,
    max_angle: f64,
) -> JniResult<Option<AimTarget>> {
    if ctx.env.is_same_object(entity, &ctx.player)?
        || !ctx.env.is_instance_of(entity, &classes.living)?
        || config.only_players && !ctx.env.is_instance_of(entity, &classes.player)?
        || !ctx.env.call_method(entity, "isAlive", "()Z", &[])?.z()?
    {
        return Ok(None);
    }

    if friends::no_friend_damage_active()
        && ctx.env.is_instance_of(entity, &classes.player)?
        && minecraft::entity_is_friend(ctx, entity)?
    {
        return Ok(None);
    }

    let health = ctx.env.call_method(entity, "getHealth", "()F", &[])?.f()? as f64;
    if health <= 0.0 {
        return Ok(None);
    }

    let invisible = ctx
        .env
        .call_method(entity, "isInvisible", "()Z", &[])?
        .z()?;
    if invisible && !config.hit_invisible {
        return Ok(None);
    }

    let armored = entity_has_armor(ctx, entity)?;
    if config.only_armored && !armored {
        return Ok(None);
    }
    if !config.only_armored && config.ignore_naked && !armored {
        return Ok(None);
    }

    let distance = ctx
        .env
        .call_method(
            &ctx.player,
            "distanceTo",
            "(Lnet/minecraft/world/entity/Entity;)F",
            &[JValue::Object(entity)],
        )?
        .f()? as f64;
    if distance > config.distance || distance < 0.5 {
        return Ok(None);
    }

    if config.wall_check && !entity_has_line_of_sight(ctx, entity)? {
        return Ok(None);
    }

    let position = entity_position(ctx, entity)?;
    let height = ctx
        .env
        .call_method(entity, "getBbHeight", "()F", &[])?
        .f()? as f64;
    let width = ctx.env.call_method(entity, "getBbWidth", "()F", &[])?.f()? as f64;
    let target_point = position.add(Vec3d {
        x: 0.0,
        y: height * config.aim_height,
        z: 0.0,
    });

    if angle_to_target(eye, view, target_point) > max_angle {
        return Ok(None);
    }

    Ok(Some(AimTarget {
        entity_id: ctx.env.call_method(entity, "getId", "()I", &[])?.i()?,
        position,
        width,
        height,
        distance,
        health,
    }))
}

fn entity_has_armor(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> JniResult<bool> {
    Ok(ctx
        .env
        .call_method(entity, "getArmorCoverPercentage", "()F", &[])?
        .f()?
        > 0.0)
}

fn entity_has_line_of_sight(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
) -> JniResult<bool> {
    ctx.env
        .call_method(
            &ctx.player,
            "hasLineOfSight",
            "(Lnet/minecraft/world/entity/Entity;)Z",
            &[JValue::Object(entity)],
        )?
        .z()
}

fn player_view_vector(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    let vector = ctx
        .env
        .call_method(
            &ctx.player,
            "getViewVector",
            "(F)Lnet/minecraft/world/phys/Vec3;",
            &[JValue::Float(1.0)],
        )?
        .l()?;
    let result = vec3_from_java(ctx.env, &vector)?;
    let _ = ctx.env.delete_local_ref(vector);
    Ok(result)
}

pub fn entity_position(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: ctx.env.call_method(entity, "getX", "()D", &[])?.d()?,
        y: ctx.env.call_method(entity, "getY", "()D", &[])?.d()?,
        z: ctx.env.call_method(entity, "getZ", "()D", &[])?.d()?,
    })
}

fn angle_to_target(eye: Vec3d, view: Vec3d, target: Vec3d) -> f64 {
    let Some(direction) = target.subtract(eye).normalize() else {
        return 0.0;
    };
    let Some(view) = view.normalize() else {
        return 180.0;
    };

    view.dot(direction).clamp(-1.0, 1.0).acos().to_degrees()
}

fn vec3_from_java(env: &mut jni::JNIEnv<'_>, vector: &JObject<'_>) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: env.get_field(vector, "x", "D")?.d()?,
        y: env.get_field(vector, "y", "D")?.d()?,
        z: env.get_field(vector, "z", "D")?.d()?,
    })
}

fn current_screen<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
) -> JniResult<Option<JObject<'local>>> {
    let screen = jni_bridge::minecraft_screen(ctx.env, &ctx.minecraft)?;
    if screen.is_null() {
        Ok(None)
    } else {
        Ok(Some(screen))
    }
}

fn java_number_as_f64(ctx: &mut MinecraftContext<'_, '_>, value: &JObject<'_>) -> Option<f64> {
    if value.is_null() {
        return None;
    }

    let number_class = jni_bridge::load_class(ctx.env, "java.lang.Number").ok()?;
    if !ctx.env.is_instance_of(value, &number_class).ok()? {
        return None;
    }

    ctx.env
        .call_method(value, "doubleValue", "()D", &[])
        .ok()?
        .d()
        .ok()
}

fn java_string(ctx: &mut MinecraftContext<'_, '_>, string: JObject<'_>) -> JniResult<String> {
    if string.is_null() {
        return Ok(String::new());
    }

    let string = JString::from(string);
    let value = ctx.env.get_string(&string)?.to_string_lossy().into_owned();
    let _ = ctx.env.delete_local_ref(JObject::from(string));
    Ok(value)
}

static OVERLAY: OnceLock<Mutex<AimAssistOverlay>> = OnceLock::new();
