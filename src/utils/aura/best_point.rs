
use crate::utils::geometry::Vec3d;

pub struct BestPoint;
impl BestPoint {
    pub fn get_nearest_point(box_min: Vec3d, box_max: Vec3d, eye: Vec3d) -> Vec3d {
        let step = 0.1;
        let mut best = Vec3d { x: (box_min.x + box_max.x) * 0.5, y: box_min.y + (box_max.y - box_min.y) * 0.7, z: (box_min.z + box_max.z) * 0.5 };
        let mut closest = f64::MAX;
        let mut x = box_min.x;
        while x <= box_max.x {
            let mut y = box_min.y;
            while y <= box_max.y {
                let mut z = box_min.z;
                while z <= box_max.z {
                    let p = Vec3d { x, y, z };
                    let d = eye.distance_to(p);
                    if d < closest { closest = d; best = p; }
                    z += step;
                }
                y += step;
            }
            x += step;
        }
        best
    }

    pub fn get_point(box_min: Vec3d, box_max: Vec3d, id: i32) -> Vec3d {
        let w = box_max.x - box_min.x;
        let h = box_max.y - box_min.y;
        let d = box_max.z - box_min.z;
        let bx = box_min.x + w * 0.5;
        let by = box_min.y + h * 0.7;
        let bz = box_min.z + d * 0.5;
        let t = (crate::utils::time::now_ms() as f64) / 50.0;
        let idf = id as f64;
        Vec3d {
            x: bx + ((t + idf).sin()) * (w * 0.45),
            y: by + ((t * 0.8 + idf).cos()) * (h * 0.1),
            z: bz + ((t * 1.2 + idf).cos()) * (d * 0.45),
        }
    }

    pub fn get_point2(box_min: Vec3d, box_max: Vec3d, id: i32) -> Vec3d {
        let w = box_max.x - box_min.x;
        let h = box_max.y - box_min.y;
        let d = box_max.z - box_min.z;
        let bx = box_min.x + w * 0.5;
        let by = box_min.y + h * 0.65;
        let bz = box_min.z + d * 0.5;
        let t = (crate::utils::time::now_ms() as f64) / 65.0;
        let idf = id as f64;
        Vec3d {
            x: bx + ((t + idf).sin()) * (w * 0.7),
            y: by + ((t * 0.8 + idf).cos()) * (h * 0.4),
            z: bz + ((t * 1.2 + idf).cos()) * (d * 0.7),
        }
    }

    
    pub fn get_multipoint(box_min: Vec3d, box_max: Vec3d, distance: f64) -> (Vec3d, Vec3d, Vec3d) {
        
        let len_x = box_max.x - box_min.x;
        let len_y = box_max.y - box_min.y;
        let len_z = box_max.z - box_min.z;
        let mut rp = Vec3d { x: 0.0, y: len_y * 0.5, z: 0.0 };
        let mut rm = Vec3d {
            x: crate::utils::aura::math::random_best(-0.02, 0.02) as f64,
            y: crate::utils::aura::math::random_best(-0.02, 0.02) as f64,
            z: crate::utils::aura::math::random_best(-0.02, 0.02) as f64,
        };
        rp.x += rm.x; rp.y += rm.y; rp.z += rm.z;
        let safe_x = (len_x - 0.1) / 2.0;
        let safe_z = (len_z - 0.1) / 2.0;
        if rp.x >= safe_x { rm.x = -crate::utils::aura::math::random_best(0.005, 0.015) as f64; }
        else if rp.x <= -safe_x { rm.x = crate::utils::aura::math::random_best(0.005, 0.015) as f64; }
        if rp.y >= len_y * 0.75 { rm.y = -crate::utils::aura::math::random_best(0.0015, 0.015) as f64; }
        else if rp.y <= len_y * 0.3 { rm.y = crate::utils::aura::math::random_best(0.0015, 0.015) as f64; }
        if rp.z >= safe_z { rm.z = -crate::utils::aura::math::random_best(0.005, 0.015) as f64; }
        else if rp.z <= -safe_z { rm.z = crate::utils::aura::math::random_best(0.005, 0.015) as f64; }
        rp.x += crate::utils::aura::math::random_best(-0.05, 0.05) as f64;
        rp.z += crate::utils::aura::math::random_best(-0.05, 0.05) as f64;
        let _ = distance;
        let world = Vec3d { x: box_min.x + rp.x, y: box_min.y + rp.y, z: box_min.z + rp.z };
        (rp, rm, world)
    }
}

