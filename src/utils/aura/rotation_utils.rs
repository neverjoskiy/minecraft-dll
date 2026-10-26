
use crate::utils::geometry::Vec3d;

pub struct RotationUtils;
impl RotationUtils {
    
    pub fn get_rotations(eye: Vec3d, point: Vec3d) -> (f32, f32) {
        let dx = point.x - eye.x;
        let dy = point.y - eye.y;
        let dz = point.z - eye.z;
        let dist = (dx * dx + dz * dz).sqrt();
        let yaw = ((dz.atan2(dx)) * (180.0 / std::f64::consts::PI)) as f32 - 90.0;
        let pitch = (-(dy.atan2(dist)) * (180.0 / std::f64::consts::PI)) as f32;
        (crate::utils::aura::math::wrap_degrees(yaw), pitch)
    }

    pub fn from_vec3d(v: Vec3d) -> Rotation2 {
        Rotation2 {
            yaw: crate::utils::aura::math::wrap_degrees(((v.z.atan2(v.x)).to_degrees() - 90.0) as f32),
            pitch: crate::utils::aura::math::wrap_degrees((-(v.y.atan2((v.x * v.x + v.z * v.z).sqrt()))).to_degrees() as f32),
        }
    }

    pub fn get_rotation_vector(yaw: f32, pitch: f32) -> Vec3d {
        let yaw_r = yaw as f64 * 0.017453292;
        let pitch_r = -pitch as f64 * 0.017453292;
        Vec3d {
            x: (-pitch_r.sin()) * yaw_r.cos(),
            y: -yaw_r.sin(),
            z: (-pitch_r.cos()) * yaw_r.cos(),
        }
    }

    pub fn get_gcd_value() -> f32 { crate::utils::aura::math::get_gcd_value() }

    pub fn correct_rotation(yaw: f32, pitch: f32) -> (f32, f32) {
        let gcd = Self::get_gcd_value();
        (yaw - yaw % gcd, pitch - pitch % gcd)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rotation2 { pub yaw: f32, pub pitch: f32 }
