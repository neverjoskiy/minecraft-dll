use super::{Mat3, Vec3, Vec4};


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f64; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat4 {
    pub fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn translation(t: Vec3) -> Self {
        let mut m = Self::identity();
        m.m[3][0] = t.x;
        m.m[3][1] = t.y;
        m.m[3][2] = t.z;
        m
    }

    pub fn scale(s: Vec3) -> Self {
        let mut m = Self::identity();
        m.m[0][0] = s.x;
        m.m[1][1] = s.y;
        m.m[2][2] = s.z;
        m
    }

    pub fn from_rotation(r: Mat3) -> Self {
        let mut m = Self::identity();
        m.m[0][0] = r.m00;
        m.m[0][1] = r.m10;
        m.m[0][2] = r.m20;
        m.m[1][0] = r.m01;
        m.m[1][1] = r.m11;
        m.m[1][2] = r.m21;
        m.m[2][0] = r.m02;
        m.m[2][1] = r.m12;
        m.m[2][2] = r.m22;
        m
    }

    pub fn mul(self, other: Self) -> Self {
        let mut out = Self { m: [[0.0; 4]; 4] };
        for i in 0..4 {
            for j in 0..4 {
                out.m[i][j] = self.m[0][j] * other.m[i][0]
                    + self.m[1][j] * other.m[i][1]
                    + self.m[2][j] * other.m[i][2]
                    + self.m[3][j] * other.m[i][3];
            }
        }
        out
    }

    #[inline]
    pub fn mul_assign(&mut self, other: Self) {
        *self = self.mul(other);
    }

    #[inline]
    pub fn is_identity(self) -> bool {
        self == Self::identity()
    }

    pub fn transform_point(self, v: Vec3) -> Vec3 {
        let (x, y, z) = self.transform_xyz(v.x, v.y, v.z);
        Vec3::new(x, y, z)
    }

    
    #[inline]
    pub fn transform_xyz(self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        (
            self.m[0][0] * x + self.m[1][0] * y + self.m[2][0] * z + self.m[3][0],
            self.m[0][1] * x + self.m[1][1] * y + self.m[2][1] * z + self.m[3][1],
            self.m[0][2] * x + self.m[1][2] * y + self.m[2][2] * z + self.m[3][2],
        )
    }

    pub fn transform(self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.m[0][0] * v.x + self.m[1][0] * v.y + self.m[2][0] * v.z + self.m[3][0] * v.w,
            self.m[0][1] * v.x + self.m[1][1] * v.y + self.m[2][1] * v.z + self.m[3][1] * v.w,
            self.m[0][2] * v.x + self.m[1][2] * v.y + self.m[2][2] * v.z + self.m[3][2] * v.w,
            self.m[0][3] * v.x + self.m[1][3] * v.y + self.m[2][3] * v.z + self.m[3][3] * v.w,
        )
    }
}
