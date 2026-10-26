use std::sync::{LazyLock, Mutex};

use jni::{JNIEnv, errors::Result as JniResult, objects::JObject, objects::JValue};

use crate::{
    jni_bridge,
    utils::{
        aim_target,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
    },
};

#[derive(Clone, Copy, Debug)]
pub struct WorldEntityMarker {
    pub center: Vec3d,
    pub min_y: f64,
    pub max_y: f64,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ChinaHatConfig {
    pub radius: f32,
    pub color: [f32; 4],
}

pub type TargetEspConfig = crate::utils::target_esp_fx::FxConfig;

static TARGET_ESP_CONFIG: LazyLock<Mutex<Option<TargetEspConfig>>> =
    LazyLock::new(|| Mutex::new(None));
static CHINA_HAT_CONFIG: LazyLock<Mutex<Option<ChinaHatConfig>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn set_target_esp_config(config: Option<TargetEspConfig>) {
    if let Ok(mut slot) = TARGET_ESP_CONFIG.lock() {
        *slot = config;
    }
}

pub fn target_esp_config() -> Option<TargetEspConfig> {
    TARGET_ESP_CONFIG.lock().ok().and_then(|slot| slot.clone())
}

pub fn clear_target_esp_config() {
    set_target_esp_config(None);
}

pub fn set_china_hat_config(config: Option<ChinaHatConfig>) {
    if let Ok(mut slot) = CHINA_HAT_CONFIG.lock() {
        *slot = config;
    }
}

pub fn china_hat_config() -> Option<ChinaHatConfig> {
    CHINA_HAT_CONFIG.lock().ok().and_then(|slot| *slot)
}

pub fn clear_china_hat_config() {
    set_china_hat_config(None);
}

fn world_marker_from_entity(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    partial_ticks: Option<f32>,
) -> JniResult<WorldEntityMarker> {
    let aabb = minecraft::entity_aabb(ctx, entity)?;
    let mut center = Vec3d {
        x: (aabb.min_x + aabb.max_x) * 0.5,
        y: (aabb.min_y + aabb.max_y) * 0.5,
        z: (aabb.min_z + aabb.max_z) * 0.5,
    };
    let radius = ((aabb.max_x - aabb.min_x)
        .abs()
        .max((aabb.max_z - aabb.min_z).abs())
        * 0.62)
        .clamp(0.34, 1.25);
    let mut min_y = aabb.min_y;
    let mut max_y = aabb.max_y;

    
    
    
    
    if let Some(partial_ticks) = partial_ticks {
        let current = Vec3d {
            x: ctx.env.call_method(entity, "getX", "()D", &[])?.d()?,
            y: ctx.env.call_method(entity, "getY", "()D", &[])?.d()?,
            z: ctx.env.call_method(entity, "getZ", "()D", &[])?.d()?,
        };
        let interp = entity_position_interpolated(ctx.env, entity, partial_ticks)?;
        center.x += interp.x - current.x;
        center.y += interp.y - current.y;
        center.z += interp.z - current.z;
        min_y += interp.y - current.y;
        max_y += interp.y - current.y;
    }

    Ok(WorldEntityMarker {
        center,
        min_y,
        max_y,
        radius,
    })
}



fn entity_position_interpolated(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    partial_ticks: f32,
) -> JniResult<Vec3d> {
    match env.call_method(
        entity,
        "getPosition",
        "(F)Lnet/minecraft/world/phys/Vec3;",
        &[JValue::Float(partial_ticks)],
    ) {
        Ok(value) => {
            let position = value.l()?;
            let result = Vec3d {
                x: env.get_field(&position, "x", "D")?.d()?,
                y: env.get_field(&position, "y", "D")?.d()?,
                z: env.get_field(&position, "z", "D")?.d()?,
            };
            let _ = env.delete_local_ref(position);
            Ok(result)
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            Ok(Vec3d {
                x: env.call_method(entity, "getX", "()D", &[])?.d()?,
                y: env.call_method(entity, "getY", "()D", &[])?.d()?,
                z: env.call_method(entity, "getZ", "()D", &[])?.d()?,
            })
        }
    }
}

fn frame_partial_ticks(ctx: &mut MinecraftContext<'_, '_>) -> f32 {
    let value = (|| -> JniResult<f32> {
        let delta_tracker = ctx
            .env
            .call_method(
                &ctx.minecraft,
                "getDeltaTracker",
                "()Lnet/minecraft/client/DeltaTracker;",
                &[],
            )?
            .l()?;
        let value = ctx
            .env
            .call_method(
                &delta_tracker,
                "getGameTimeDeltaPartialTick",
                "(Z)F",
                &[JValue::Bool(0)],
            )?
            .f()?;
        let _ = ctx.env.delete_local_ref(delta_tracker);
        Ok(value)
    })();

    match value {
        Ok(value) if value.is_finite() => value.clamp(0.0, 1.0),
        _ => {
            jni_bridge::clear_pending_exception(ctx.env);
            1.0
        }
    }
}









pub fn build_world_geometry(
    env: &mut JNIEnv<'_>,
    lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) -> JniResult<()> {
    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(());
    }
    let partial_ticks = frame_partial_ticks(&mut ctx);

    if let Some(config) = target_esp_config() {
        let overlay = aim_target::overlay_state();
        if overlay.target_active
            && let Some(entity_id) = overlay.target_entity_id
            && let Some(entity) = ctx.entity_by_id(entity_id)?
        {
            let marker = world_marker_from_entity(&mut ctx, &entity, Some(partial_ticks))?;
            let hurt_time = ctx
                .env
                .get_field(&entity, "hurtTime", "I")
                .and_then(|v| v.i())
                .unwrap_or(0);
            let _ = ctx.env.delete_local_ref(entity);
            let hurt_pc = crate::utils::target_esp_fx::hurt_progress(
                hurt_time,
                partial_ticks,
                config.hurt_color,
            );
            crate::utils::target_esp_fx::emit(
                lines,
                filled,
                camera,
                marker,
                &config,
                hurt_pc,
                true,
            );
        }
    }

    if let Some(config) = china_hat_config()
        && !camera_is_first_person(&mut ctx)
    {
        
        
        
        
        
        let player = ctx.env.new_local_ref(&ctx.player)?;
        let marker = world_marker_from_entity(&mut ctx, &player, Some(partial_ticks))?;
        let _ = ctx.env.delete_local_ref(player);
        emit_china_hat(lines, filled, camera, marker, config.radius, config.color);
    }
    Ok(())
}





fn emit_china_hat(
    lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
    marker: WorldEntityMarker,
    setting_radius: f32,
    color: [f32; 4],
) {
    use crate::world_render::geometry;

    let radius = f64::from(setting_radius).clamp(0.5, 1.8) * marker.radius * 1.45;
    let base = Vec3d {
        x: marker.center.x,
        y: marker.max_y + 0.06,
        z: marker.center.z,
    };
    let apex = Vec3d {
        x: base.x,
        y: base.y + radius * 0.62,
        z: base.z,
    };
    geometry::push_cone(filled, camera, apex, base, radius, 32, color);

    
    let alpha = color[3].clamp(0.0, 1.0);
    let tint = |mul: f32| {
        [
            color[0],
            color[1],
            color[2],
            (alpha * mul).clamp(0.0, 1.0),
        ]
    };
    let rim_points = geometry::circle_points(base, radius, 48);
    geometry::push_ring_width(lines, camera, &rim_points, tint(0.18), 3.0);
    geometry::push_ring_width(lines, camera, &rim_points, tint(0.92), 1.5);

    
    
    let highlight = [1.0, 1.0, 1.0, (alpha * 0.35).clamp(0.0, 1.0)];
    let spokes = geometry::circle_points(base, radius, 32);
    for index in (0..spokes.len()).step_by(4) {
        geometry::push_segment(lines, camera, apex, spokes[index], highlight);
    }
}



fn spin_phase(speed: f32) -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let elapsed = start.elapsed().as_secs_f64();
    (elapsed * f64::from(speed.max(0.0))) % std::f64::consts::TAU
}

fn camera_is_first_person(ctx: &mut MinecraftContext<'_, '_>) -> bool {
    let result = (|| -> JniResult<bool> {
        let camera_type = ctx
            .env
            .call_method(
                &ctx.options,
                "getCameraType",
                "()Lnet/minecraft/client/CameraType;",
                &[],
            )?
            .l()?;
        let first_person = ctx
            .env
            .call_method(&camera_type, "isFirstPerson", "()Z", &[])?
            .z()?;
        let _ = ctx.env.delete_local_ref(camera_type);
        Ok(first_person)
    })();

    match result {
        Ok(first_person) => first_person,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            false
        }
    }
}
