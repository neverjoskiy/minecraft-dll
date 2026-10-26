
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub fn new(yaw: f32, pitch: f32) -> Self { Self { yaw, pitch } }

    pub fn get_delta(&self, target: &Rotation) -> f32 {
        let yaw_delta = crate::utils::aura::math::wrap_degrees(target.yaw - self.yaw);
        let pitch_delta = target.pitch - self.pitch;
        ((yaw_delta.abs() as f64).hypot(pitch_delta.abs() as f64)) as f32
    }

    pub fn get_delta_double(&self, target: &Rotation) -> f64 {
        let yaw_delta = crate::utils::aura::math::wrap_degrees(target.yaw - self.yaw) as f64;
        let pitch_delta = crate::utils::aura::math::wrap_degrees(target.pitch - self.pitch) as f64;
        yaw_delta.hypot(pitch_delta)
    }

    
    pub fn to_vector(&self) -> crate::utils::geometry::Vec3d {
        let f = self.pitch as f64 * 0.017453292;
        let g = -(self.yaw as f64) * 0.017453292;
        let h = g.cos();
        let i = g.sin();
        let j = f.cos();
        let k = f.sin();
        crate::utils::geometry::Vec3d { x: i * j, y: -k, z: h * j }
    }

    
    pub fn from_positions(player_pos: crate::utils::geometry::Vec3d, target_pos: crate::utils::geometry::Vec3d) -> Self {
        let dx = target_pos.x - player_pos.x;
        let dy = target_pos.y - player_pos.y;
        let dz = target_pos.z - player_pos.z;
        let distance_xz = (dx * dx + dz * dz).sqrt();
        let yaw = ((dz.atan2(dx)).to_degrees() - 90.0) as f32;
        let pitch = (-(dy.atan2(distance_xz)).to_degrees()) as f32;
        Self { yaw, pitch }
    }
}
