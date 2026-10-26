

use std::sync::{LazyLock, Mutex};

use crate::{utils::geometry::Vec3d, world_render::geometry};

pub const MODE_IMAGE1: &str = "Картинка 1";
pub const MODE_IMAGE2: &str = "Картинка 2";
pub const MODE_RING: &str = "Кольцо";
pub const MODE_SOULS: &str = "Души";
pub const MODE_CUBES: &str = "Кубы";
pub const MODE_CRYSTALS: &str = "Кристаллы";
pub const MODE_RIDER: &str = "Райдер";
pub const MODE_GHOSTS: &str = "Призраки";

const GHOST_ALPHA_MULT: f32 = 0.6;
const CELKA_SPEED_MULT: f32 = 1.2;
const SCALE_FACTOR: f64 = 0.007;

#[derive(Clone, Debug)]
pub struct FxConfig {
    pub mode: String,
    pub size: f32,
    pub rotate_speed: f32,
    pub hurt_color: bool,
    pub ring_radius: f32,
    pub ring_speed: f32,
    pub ghost_count: i32,
    pub ghost_life_ms: u64,
    pub strength_xz: f32,
    pub strength_y: f32,
    pub color: [f32; 4],
}

struct FxState {
    appear: f32,
    scale: f32,
    rot_progress: f32,
    rot_from: f32,
    rot_to: f32,
    last_rotate_update: u64,
    bmw_points: Vec<GlowPoint>,
    crystal_rotation: f32,
    crystal_animation: f32,
}

impl FxState {
    fn new() -> Self {
        Self { appear: 0.0, scale: 0.0, rot_progress: 0.0, rot_from: -280.0, rot_to: 280.0, last_rotate_update: 0, bmw_points: Vec::new(), crystal_rotation: 0.0, crystal_animation: 0.0 }
    }
}

struct GlowPoint { x: f64, y: f64, z: f64, start_ms: u64, max_life: u64 }

static FX: LazyLock<Mutex<FxState>> = LazyLock::new(|| Mutex::new(FxState::new()));

fn animate_to(current: f32, target: f32, delta: f32) -> f32 {
    if current < target { (current + delta).min(target) } else if current > target { (current - delta).max(target) } else { current }
}
fn sine_in_out(t: f32) -> f32 { -((std::f32::consts::PI * t).cos()) / 2.0 + 0.5 }
fn over_color(base: [f32;4], hurt: [f32;4], f: f32) -> [f32;4] {
    let f = f.clamp(0.0, 1.0);
    [base[0] + (hurt[0]-base[0])*f, base[1] + (hurt[1]-base[1])*f, base[2] + (hurt[2]-base[2])*f, base[3] + (hurt[3]-base[3])*f]
}
fn now_ms() -> u64 { crate::utils::time::now_ms() }
fn screen_to_world(screen: f32, dist: f64) -> f64 { screen as f64 * dist.max(0.1) * SCALE_FACTOR }
pub fn hurt_progress(hurt_time: i32, partial: f32, enabled: bool) -> f32 {
    if !enabled { return 0.0; }
    let v = (hurt_time as f32 - partial).clamp(0.0, 10.0);
    let p = v / 10.0;
    p * p * (3.0 - 2.0 * p)
}

pub fn emit(
    lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
    marker: crate::utils::world_markers::WorldEntityMarker,
    cfg: &FxConfig,
    hurt_pc: f32,
    has_target: bool,
) {
    let mut state = match FX.lock() { Ok(g) => g, Err(_) => return };
    let now = now_ms();
    state.appear = animate_to(state.appear, if has_target {1.0} else {0.0}, 0.05);
    state.scale = animate_to(state.scale, if has_target {1.0} else {0.5}, 0.05);
    if has_target && cfg.mode == MODE_CRYSTALS {
        state.crystal_animation = animate_to(state.crystal_animation, 1.0, 0.07);
        state.crystal_rotation += 0.8;
    }
    if state.appear <= 0.001 && !has_target && (cfg.mode != MODE_CRYSTALS || state.crystal_animation <= 0.001) {
        return;
    }
    let entity_height = (marker.max_y - marker.min_y).max(0.5);
    let base = Vec3d { x: marker.center.x, y: marker.min_y, z: marker.center.z };
    let appear = state.appear;
    let base_col = cfg.color;
    let hurt_col = [1.0, 0.0117, 0.0117, 1.0];
    let color = over_color([base_col[0], base_col[1], base_col[2], base_col[3]*appear], [hurt_col[0], hurt_col[1], hurt_col[2], hurt_col[3]*appear], hurt_pc);

    match cfg.mode.as_str() {
        MODE_IMAGE1 | MODE_IMAGE2 => {
            
            if state.last_rotate_update == 0 { state.last_rotate_update = now; }
            let dt = ((now - state.last_rotate_update) as f32 / 1000.0).max(0.001);
            state.last_rotate_update = now;
            let cycle = (2.2 / cfg.rotate_speed.max(0.05)).max(0.35);
            state.rot_progress += dt / cycle;
            while state.rot_progress >= 1.0 { state.rot_progress -= 1.0; state.rot_from = state.rot_to; state.rot_to = if state.rot_to > 0.0 { -280.0 } else { 280.0 }; }
            let eased = sine_in_out(state.rot_progress);
            let rot = eased * (state.rot_to - state.rot_from) + state.rot_from;
            let world_y = base.y + (entity_height + 0.4) * 0.5;
            let center = Vec3d { x: base.x, y: world_y, z: base.z };
            let dist = camera.distance_to(center);
            let world_size = screen_to_world(cfg.size * 12.0 * state.scale, dist);
            
            let rad = (rot as f64).to_radians();
            let (s, c) = rad.sin_cos();
            let half = world_size * 0.5;
            
            geometry::push_billboard_quad(filled, camera, center, half*2.0, half*2.0, color);
            let _ = s; let _ = c;
        },
        MODE_RING => {
            let duration = (2000.0 / cfg.ring_speed.max(0.05)) as u64;
            let elapsed = now % duration.max(1);
            let side = elapsed > duration/2;
            let mut progress = elapsed as f64 / (duration as f64 /2.0);
            if side { progress -= 1.0; } else { progress = 1.0 - progress; }
            let progress = if progress < 0.5 { 2.0*progress*progress } else { 1.0 - (-2.0*progress+2.0).powi(2)/2.0 };
            let eased = entity_height / 1.2 * (if progress>0.5 {1.0-progress} else {progress}) * if side {-1.0} else {1.0};
            let y_band = base.y + entity_height * progress;
            let y_band2 = y_band + eased;
            let radius = cfg.ring_radius as f64;
            let center = Vec3d { x: base.x, y: y_band, z: base.z };
            let center2 = Vec3d { x: base.x, y: y_band2, z: base.z };
            let points = geometry::circle_points(center, radius, 48);
            let points2 = geometry::circle_points(center2, radius, 48);
            for w in points.windows(2) {
                let a = Vec3d { x: w[0].x, y: y_band, z: w[0].z };
                let b = Vec3d { x: w[1].x, y: y_band, z: w[1].z };
                let c = Vec3d { x: w[1].x, y: y_band2, z: w[1].z };
                let d = Vec3d { x: w[0].x, y: y_band2, z: w[0].z };
                geometry::push_quad(filled, camera, [a,b,c,d], [color[0], color[1], color[2], 0.88*appear]);
            }
            let ring: Vec<Vec3d> = points.iter().map(|p| Vec3d { x: p.x, y: y_band, z: p.z }).collect();
            geometry::push_ring_width(lines, camera, &ring, color, 1.5);
            let _ = points2;
        },
        MODE_SOULS => {
            let time = now as f64;
            for orbit in 0..3 {
                for i in 0..20 {
                    let trail = 1.0 - i as f32 /20.0 *0.7;
                    let angle = 0.15*(time - i as f64*10.0)/25.0;
                    let s = angle.sin()*0.7; let c = angle.cos()*0.7;
                    let (wx,wy,wz) = match orbit { 0 => (base.x + s, base.y + entity_height/2.0 + c, base.z - c), 1 => (base.x - s, base.y + entity_height/2.0 + s, base.z - c), _ => (base.x - s, base.y + entity_height/2.0 - s, base.z + c) };
                    let pos = Vec3d { x: wx, y: wy, z: wz };
                    let dist = camera.distance_to(pos);
                    let col = over_color([base_col[0], base_col[1], base_col[2], appear*GHOST_ALPHA_MULT], [hurt_col[0], hurt_col[1], hurt_col[2], appear*GHOST_ALPHA_MULT], hurt_pc);
                    let sz = screen_to_world(4.0*trail*0.12, dist);
                    geometry::push_billboard_quad(filled, camera, pos, sz, sz, col);
                }
            }
        },
        MODE_CUBES | MODE_RIDER | MODE_GHOSTS | _ => {
            
            let time = now as f64;
            let count = if cfg.mode == MODE_CUBES { 12 } else { cfg.ghost_count.max(1) as usize * 4 };
            for i in 0..count {
                let angle = (i as f64 / count as f64)*std::f64::consts::TAU + time*0.002*cfg.rotate_speed as f64;
                let r = if cfg.mode == MODE_RING { cfg.ring_radius as f64 } else { 0.7 };
                let x = base.x + angle.cos()*r;
                let z = base.z + angle.sin()*r;
                let y = base.y + (angle*0.5).sin()*0.3 + entity_height*0.5;
                let pos = Vec3d { x, y, z };
                let dist = camera.distance_to(pos);
                let sz = screen_to_world(cfg.size*2.0, dist);
                geometry::push_billboard_quad(filled, camera, pos, sz, sz, color);
            }
        }
    }
}
