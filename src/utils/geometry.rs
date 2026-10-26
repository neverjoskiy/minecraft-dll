#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3d {
    pub fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    pub fn subtract(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    pub fn scale(self, amount: f64) -> Self {
        Self {
            x: self.x * amount,
            y: self.y * amount,
            z: self.z * amount,
        }
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalize(self) -> Option<Self> {
        let length = self.length();
        if length < 0.001 {
            return None;
        }

        Some(self.scale(1.0 / length))
    }

    pub fn distance_to(self, other: Self) -> f64 {
        self.distance_to_sqr(other).sqrt()
    }

    pub fn distance_to_sqr(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    pub fn inflate(self, amount: f64) -> Self {
        Self {
            min_x: self.min_x - amount,
            min_y: self.min_y - amount,
            min_z: self.min_z - amount,
            max_x: self.max_x + amount,
            max_y: self.max_y + amount,
            max_z: self.max_z + amount,
        }
    }

    pub fn contains(self, point: Vec3d) -> bool {
        point.x >= self.min_x
            && point.x <= self.max_x
            && point.y >= self.min_y
            && point.y <= self.max_y
            && point.z >= self.min_z
            && point.z <= self.max_z
    }

    pub fn distance_to_point(self, point: Vec3d) -> f64 {
        let x = point.x.clamp(self.min_x, self.max_x);
        let y = point.y.clamp(self.min_y, self.max_y);
        let z = point.z.clamp(self.min_z, self.max_z);
        point.distance_to(Vec3d { x, y, z })
    }

    pub fn ray_intersection(self, from: Vec3d, to: Vec3d) -> Option<Vec3d> {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        let mut t_min = 0.0;
        let mut t_max = 1.0;

        if !clip_axis(from.x, dx, self.min_x, self.max_x, &mut t_min, &mut t_max) {
            return None;
        }
        if !clip_axis(from.y, dy, self.min_y, self.max_y, &mut t_min, &mut t_max) {
            return None;
        }
        if !clip_axis(from.z, dz, self.min_z, self.max_z, &mut t_min, &mut t_max) {
            return None;
        }

        Some(Vec3d {
            x: from.x + dx * t_min,
            y: from.y + dy * t_min,
            z: from.z + dz * t_min,
        })
    }
}

fn clip_axis(
    origin: f64,
    delta: f64,
    min: f64,
    max: f64,
    t_min: &mut f64,
    t_max: &mut f64,
) -> bool {
    if delta.abs() < f64::EPSILON {
        return origin >= min && origin <= max;
    }

    let inv_delta = 1.0 / delta;
    let mut near = (min - origin) * inv_delta;
    let mut far = (max - origin) * inv_delta;
    if near > far {
        std::mem::swap(&mut near, &mut far);
    }

    *t_min = (*t_min).max(near);
    *t_max = (*t_max).min(far);
    *t_min <= *t_max
}
