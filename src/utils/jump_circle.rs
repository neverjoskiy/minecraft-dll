use std::sync::Mutex;
use std::time::Instant;

use jni::errors::Result as JniResult;

use crate::{
    interface::Module,
    jni_bridge,
    utils::{
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        settings,
    },
};

pub const LIFETIME_SETTING: &str = "Время жизни";
pub const SPEED_SETTING: &str = "Скорость";
pub const THEME_SYNC_SETTING: &str = "Синхронизация тем";



const SEGMENTS_3D: usize = 64;


const CORE_WIDTH: f32 = 3.0;



const BLOOM_BANDS: [(f64, f32, f32); 3] = [(1.06, 0.16, 2.0), (1.13, 0.10, 2.0), (1.21, 0.06, 2.0)];


const GLOW_DISCS: [(f64, f32); 3] = [(1.18, 0.05), (0.85, 0.08), (0.52, 0.12)];
const DEFAULT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DEFAULT_SIZE: f64 = 1.0;

const BASE_EXPAND: f64 = 0.35;

#[derive(Clone, Copy)]
struct ActiveCircle {
    origin: Vec3d,
    born: Instant,
}

struct RuntimeState {
    was_on_ground: bool,
    circles: Vec<ActiveCircle>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            was_on_ground: true,
            circles: Vec::new(),
        }
    }
}

static STATE: Mutex<RuntimeState> = Mutex::new(RuntimeState {
    was_on_ground: true,
    circles: Vec::new(),
});

pub fn clear() {
    if let Ok(mut state) = STATE.lock() {
        *state = RuntimeState::default();
    }
}




pub fn tick(config: &Module) {
    if !config.enabled {
        clear();
        return;
    }

    let lifetime = settings::slider(config, LIFETIME_SETTING, 0.9).max(0.05) as f64;
    let sync_theme = settings::checkbox(config, THEME_SYNC_SETTING, true);
    let color = if sync_theme {
        let theme = crate::theme_colors::current();
        [theme.accent[0], theme.accent[1], theme.accent[2], 1.0]
    } else {
        DEFAULT_COLOR
    };
    if let Ok(mut params) = PARAMS.lock() {
        *params = RingParams {
            lifetime,
            speed: settings::slider(config, SPEED_SETTING, 1.0).max(0.05) as f64,
            color,
        };
    }

    let _ = jni_bridge::with_env(|env| {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(());
        }
        update_state(&mut ctx, lifetime)?;
        Ok(())
    });
}

#[derive(Clone, Copy)]
struct RingParams {
    lifetime: f64,
    speed: f64,
    color: [f32; 4],
}

impl Default for RingParams {
    fn default() -> Self {
        Self {
            lifetime: 0.9,
            speed: 1.0,
            color: DEFAULT_COLOR,
        }
    }
}

static PARAMS: Mutex<RingParams> = Mutex::new(RingParams {
    lifetime: 0.9,
    speed: 1.0,
    color: DEFAULT_COLOR,
});


fn update_state(ctx: &mut MinecraftContext<'_, '_>, duration: f64) -> JniResult<()> {
    let on_ground = minecraft::player_on_ground(ctx)?;
    let velocity_y = minecraft::player_velocity_y(ctx)?;
    let position = player_position(ctx)?;

    let now = Instant::now();
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.was_on_ground && !on_ground && velocity_y > 0.05 {
        state.circles.push(ActiveCircle {
            origin: Vec3d {
                x: position.x,
                y: position.y + 0.02,
                z: position.z,
            },
            born: now,
        });
    }
    state.was_on_ground = on_ground;
    state
        .circles
        .retain(|circle| now.duration_since(circle.born).as_secs_f64() < duration);
    Ok(())
}




pub fn build_world_geometry(
    lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) {
    let params = PARAMS.lock().map(|p| *p).unwrap_or_default();
    let Ok(state) = STATE.lock() else {
        return;
    };
    let now = Instant::now();
    let expand = BASE_EXPAND * params.speed;
    for circle in &state.circles {
        let age = now.duration_since(circle.born).as_secs_f64();
        let progress = (age / params.lifetime).clamp(0.0, 1.0);
        let radius = DEFAULT_SIZE * (1.0 + expand * progress);
        let fade = ((1.0 - progress) as f32).clamp(0.0, 1.0) * params.color[3].clamp(0.0, 1.0);
        if fade <= 0.01 {
            continue;
        }
        let tint = |alpha_mul: f32| {
            [
                params.color[0],
                params.color[1],
                params.color[2],
                (fade * alpha_mul).clamp(0.0, 1.0),
            ]
        };

        
        
        
        for (scale, alpha_mul) in GLOW_DISCS {
            crate::world_render::geometry::push_ground_quad(
                filled,
                camera,
                circle.origin,
                radius * scale,
                0.0,
                tint(alpha_mul),
            );
        }

        
        for (scale, alpha_mul, width) in BLOOM_BANDS {
            let points =
                crate::world_render::geometry::circle_points(circle.origin, radius * scale, SEGMENTS_3D);
            crate::world_render::geometry::push_ring_width(
                lines,
                camera,
                &points,
                tint(alpha_mul),
                width,
            );
        }

        
        let points = crate::world_render::geometry::circle_points(circle.origin, radius, SEGMENTS_3D);
        crate::world_render::geometry::push_ring_width(
            lines,
            camera,
            &points,
            tint(1.0),
            CORE_WIDTH,
        );
    }
}

fn player_position(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: ctx.env.call_method(&ctx.player, "getX", "()D", &[])?.d()?,
        y: ctx.env.call_method(&ctx.player, "getY", "()D", &[])?.d()?,
        z: ctx.env.call_method(&ctx.player, "getZ", "()D", &[])?.d()?,
    })
}
