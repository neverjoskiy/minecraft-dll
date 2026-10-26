use crate::utils::geometry::Vec3d;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
struct PositionData {
    server_x: f64, server_y: f64, server_z: f64,
    prev_x: f64, prev_y: f64, prev_z: f64,
    last_speed: f64,
    last_update: u64,
}

impl PositionData {
    fn forward(&self) -> Vec3d {
        Vec3d { x: self.server_x - self.prev_x, y: self.server_y - self.prev_y, z: self.server_z - self.prev_z }
    }
}

static CACHE: OnceLock<Mutex<HashMap<Uuid, PositionData>>> = OnceLock::new();
fn cache() -> &'static Mutex<HashMap<Uuid, PositionData>> { CACHE.get_or_init(|| Mutex::new(HashMap::new())) }

pub struct Predict;
impl Predict {
    pub fn update_entity(uuid: Uuid, x: f64, y: f64, z: f64) {
        if let Ok(mut c) = cache().lock() {
            let e = c.entry(uuid).or_insert(PositionData{server_x:x, server_y:y, server_z:z, prev_x:x, prev_y:y, prev_z:z, last_speed:0.0, last_update:0});
            let prev = *e;
            e.prev_x = prev.server_x; e.prev_y = prev.server_y; e.prev_z = prev.server_z;
            e.server_x = x; e.server_y = y; e.server_z = z;
            let fwd = e.forward(); e.last_speed = (fwd.x*fwd.x+fwd.y*fwd.y+fwd.z*fwd.z).sqrt()*20.0;
            e.last_update = crate::utils::time::now_ms();
        }
    }
    pub fn predict(pos: Vec3d, velocity: Vec3d, ticks: i32) -> Vec3d {
        
        Vec3d { x: pos.x + velocity.x * ticks as f64, y: pos.y + velocity.y * ticks as f64, z: pos.z + velocity.z * ticks as f64 }
    }
    pub fn predict_elytra(pos: Vec3d, velocity: Vec3d, ticks: i32) -> Vec3d {
        let mut p = pos; let mut v = velocity;
        for _ in 0..ticks {
            v.y += -0.08 * (-1.0 + 0.0) * 0.75;
            v.x *= 0.99; v.y *= 0.98; v.z *= 0.99;
            p.x += v.x; p.y += v.y; p.z += v.z;
        }
        p
    }
    pub fn bypass_elytra(relative: Vec3d, forward: f64) -> Vec3d {
        Vec3d { x: relative.x + forward*0.5, y: relative.y, z: relative.z + forward*0.5 }
    }
}
