use super::Vec3;


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub m00: f64,
    pub m01: f64,
    pub m02: f64,
    pub m10: f64,
    pub m11: f64,
    pub m12: f64,
    pub m20: f64,
    pub m21: f64,
    pub m22: f64,
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat3 {
    pub const fn identity() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m02: 0.0,
            m10: 0.0,
            m11: 1.0,
            m12: 0.0,
            m20: 0.0,
            m21: 0.0,
            m22: 1.0,
        }
    }

    pub fn mul(self, other: Self) -> Self {
        Self {
            m00: self.m00 * other.m00 + self.m01 * other.m10 + self.m02 * other.m20,
            m01: self.m00 * other.m01 + self.m01 * other.m11 + self.m02 * other.m21,
            m02: self.m00 * other.m02 + self.m01 * other.m12 + self.m02 * other.m22,
            m10: self.m10 * other.m00 + self.m11 * other.m10 + self.m12 * other.m20,
            m11: self.m10 * other.m01 + self.m11 * other.m11 + self.m12 * other.m21,
            m12: self.m10 * other.m02 + self.m11 * other.m12 + self.m12 * other.m22,
            m20: self.m20 * other.m00 + self.m21 * other.m10 + self.m22 * other.m20,
            m21: self.m20 * other.m01 + self.m21 * other.m11 + self.m22 * other.m21,
            m22: self.m20 * other.m02 + self.m21 * other.m12 + self.m22 * other.m22,
        }
    }

    pub fn transform(self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.m00 * v.x + self.m01 * v.y + self.m02 * v.z,
            self.m10 * v.x + self.m11 * v.y + self.m12 * v.z,
            self.m20 * v.x + self.m21 * v.y + self.m22 * v.z,
        )
    }

    pub fn from_euler_xyz_deg(rx: f64, ry: f64, rz: f64) -> Self {
        let (sx, cx) = rx.to_radians().sin_cos();
        let (sy, cy) = ry.to_radians().sin_cos();
        let (sz, cz) = rz.to_radians().sin_cos();
        let rx_m = Self {
            m00: 1.0,
            m01: 0.0,
            m02: 0.0,
            m10: 0.0,
            m11: cx,
            m12: -sx,
            m20: 0.0,
            m21: sx,
            m22: cx,
        };
        let ry_m = Self {
            m00: cy,
            m01: 0.0,
            m02: sy,
            m10: 0.0,
            m11: 1.0,
            m12: 0.0,
            m20: -sy,
            m21: 0.0,
            m22: cy,
        };
        let rz_m = Self {
            m00: cz,
            m01: -sz,
            m02: 0.0,
            m10: sz,
            m11: cz,
            m12: 0.0,
            m20: 0.0,
            m21: 0.0,
            m22: 1.0,
        };
        
        rz_m.mul(ry_m).mul(rx_m)
    }

    
    pub fn from_euler_zyx_deg(rx: f64, ry: f64, rz: f64) -> Self {
        
        const EPS: f64 = 1e-9;
        let ax = rx.abs() >= EPS;
        let ay = ry.abs() >= EPS;
        let az = rz.abs() >= EPS;
        match (ax, ay, az) {
            (false, false, false) => Self::identity(),
            (true, false, false) => {
                let (s, c) = rx.to_radians().sin_cos();
                Self {
                    m00: 1.0,
                    m01: 0.0,
                    m02: 0.0,
                    m10: 0.0,
                    m11: c,
                    m12: -s,
                    m20: 0.0,
                    m21: s,
                    m22: c,
                }
            }
            (false, true, false) => {
                let (s, c) = ry.to_radians().sin_cos();
                Self {
                    m00: c,
                    m01: 0.0,
                    m02: s,
                    m10: 0.0,
                    m11: 1.0,
                    m12: 0.0,
                    m20: -s,
                    m21: 0.0,
                    m22: c,
                }
            }
            (false, false, true) => {
                let (s, c) = rz.to_radians().sin_cos();
                Self {
                    m00: c,
                    m01: -s,
                    m02: 0.0,
                    m10: s,
                    m11: c,
                    m12: 0.0,
                    m20: 0.0,
                    m21: 0.0,
                    m22: 1.0,
                }
            }
            _ => Self::from_euler_xyz_deg(rx, ry, rz),
        }
    }
}
