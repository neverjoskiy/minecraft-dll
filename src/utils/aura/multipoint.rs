use crate::utils::geometry::Vec3d;

pub struct Multipoint;
impl Multipoint {
    pub fn get_closest_point(eye: Vec3d, min: Vec3d, max: Vec3d) -> Vec3d {
        let step = 0.15;
        let mut best = Vec3d { x: (min.x+max.x)*0.5, y: min.y + (max.y-min.y)*0.7, z: (min.z+max.z)*0.5 };
        let mut best_dist = f64::MAX;
        let mut x = min.x;
        while x <= max.x {
            let mut y = min.y;
            while y <= max.y {
                let mut z = min.z;
                while z <= max.z {
                    let p = Vec3d { x, y, z };
                    let d = eye.distance_to(p);
                    if d < best_dist { best_dist = d; best = p; }
                    z += step;
                }
                y += step;
            }
            x += step;
        }
        best
    }
}
