use std::sync::{Mutex, OnceLock};
use crate::utils::aura::rotation::Rotation;

static STORAGE: OnceLock<Mutex<RotationStorage>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default)]
struct RotationStorage {
    target: Option<Rotation>,
    free_yaw: f32,
    free_pitch: f32,
}

fn storage() -> &'static Mutex<RotationStorage> {
    STORAGE.get_or_init(|| Mutex::new(RotationStorage::default()))
}


pub fn update(rotation: Rotation, client_look: bool, ctx_yaw: f32, ctx_pitch: f32) {
    if let Ok(mut s) = storage().lock() {
        s.target = Some(rotation);
        if client_look {
            s.free_yaw = rotation.yaw;
            s.free_pitch = rotation.pitch;
        } else {
            
            s.free_yaw = ctx_yaw;
            s.free_pitch = ctx_pitch;
        }
    }
}

pub fn get_target() -> Option<Rotation> {
    storage().lock().ok().and_then(|s| s.target)
}

pub fn get_free_yaw() -> f32 {
    storage().lock().ok().map(|s| s.free_yaw).unwrap_or(0.0)
}

pub fn get_free_pitch() -> f32 {
    storage().lock().ok().map(|s| s.free_pitch).unwrap_or(0.0)
}

pub fn clear() {
    if let Ok(mut s) = storage().lock() {
        s.target = None;
    }
}
