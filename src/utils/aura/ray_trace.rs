
use crate::utils::geometry::Vec3d;

pub fn calculate_view_vector(yaw: f32, pitch: f32) -> Vec3d {
    let pitch_rad = (pitch as f64) * 0.017453292519943295;
    let yaw_rad = -(yaw as f64) * 0.017453292519943295;
    let cos_yaw = yaw_rad.cos();
    let sin_yaw = yaw_rad.sin();
    let cos_pitch = pitch_rad.cos();
    let sin_pitch = pitch_rad.sin();
    Vec3d { x: sin_yaw * cos_pitch, y: -sin_pitch, z: cos_yaw * cos_pitch }
}

pub fn get_vector_for_rotation(pitch: f32, yaw: f32) -> Vec3d {
    let yaw_rad = -(yaw as f64) * (std::f64::consts::PI / 180.0) - std::f64::consts::PI;
    let pitch_rad = -(pitch as f64) * (std::f64::consts::PI / 180.0);
    let cos_yaw = yaw_rad.cos();
    let sin_yaw = yaw_rad.sin();
    let cos_pitch = -pitch_rad.cos();
    let sin_pitch = pitch_rad.sin();
    Vec3d { x: sin_yaw * cos_pitch, y: sin_pitch, z: cos_yaw * cos_pitch }
}


pub fn box_raycast(box_min: Vec3d, box_max: Vec3d, from: Vec3d, to: Vec3d) -> Option<Vec3d> {
    let dir = Vec3d { x: to.x - from.x, y: to.y - from.y, z: to.z - from.z };
    let mut tmin = 0.0_f64;
    let mut tmax = 1.0_f64;
    for (o, d, mn, mx) in [(from.x, dir.x, box_min.x, box_max.x), (from.y, dir.y, box_min.y, box_max.y), (from.z, dir.z, box_min.z, box_max.z)] {
        if d.abs() < 1e-9 {
            if o < mn || o > mx { return None; }
        } else {
            let t1 = (mn - o) / d;
            let t2 = (mx - o) / d;
            let (t1, t2) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax { return None; }
        }
    }
    Some(Vec3d { x: from.x + dir.x * tmin, y: from.y + dir.y * tmin, z: from.z + dir.z * tmin })
}

pub fn box_contains(box_min: Vec3d, box_max: Vec3d, point: Vec3d) -> bool {
    point.x >= box_min.x && point.x <= box_max.x && point.y >= box_min.y && point.y <= box_max.y && point.z >= box_min.z && point.z <= box_max.z
}


pub fn ray_trace_box(camera: Vec3d, client_vec: Vec3d, range: f64, box_min: Vec3d, box_max: Vec3d) -> bool {
    if box_contains(box_min, box_max, camera) { return true; }
    let end = Vec3d { x: camera.x + client_vec.x * range, y: camera.y + client_vec.y * range, z: camera.z + client_vec.z * range };
    box_raycast(box_min, box_max, camera, end).is_some()
}

pub fn ray_trace_single_entity(eye: Vec3d, yaw: f32, pitch: f32, distance: f64, box_min: Vec3d, box_max: Vec3d) -> bool {
    let look = calculate_view_vector(yaw, pitch);
    let extended = Vec3d { x: eye.x + look.x * distance, y: eye.y + look.y * distance, z: eye.z + look.z * distance };
    box_contains(box_min, box_max, eye) || box_raycast(box_min, box_max, eye, extended).is_some()
}
