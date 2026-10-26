use crate::avatar::animation::{Keyframe, TransformType};
use crate::avatar::math::{Vec3, bezier, bezier_find_t, catmullrom, lerp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Catmullrom,
    Bezier,
    Step,
}

impl Interpolation {
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "catmullrom" | "catmull" | "smooth" => Self::Catmullrom,
            "bezier" => Self::Bezier,
            "step" => Self::Step,
            _ => Self::Linear,
        }
    }

    pub fn generate(
        self,
        frames: &[Keyframe],
        current: usize,
        target: usize,
        strength: f32,
        delta: f32,
        ty: TransformType,
    ) -> Vec3 {
        let result = match self {
            Self::Linear => {
                let prev = frames[current].target_b();
                let next = frames[target].target_a();
                Vec3::new(
                    lerp(delta as f64, prev.x, next.x),
                    lerp(delta as f64, prev.y, next.y),
                    lerp(delta as f64, prev.z, next.z),
                )
            }
            Self::Catmullrom => {
                let prev = &frames[current];
                let next = &frames[target];
                let mut prev_prev_i = current.saturating_sub(1);
                let mut next_next_i = (target + 1).min(frames.len() - 1);
                if prev_prev_i == current {
                    prev_prev_i = frames.len() - 1;
                }
                if next_next_i == target {
                    next_next_i = 0;
                }
                let p0 = frames[prev_prev_i].target_b();
                let p1 = prev.target_b();
                let p2 = next.target_a();
                let p3 = frames[next_next_i].target_a();
                let d = delta as f64;
                Vec3::new(
                    catmullrom(d, p0.x, p1.x, p2.x, p3.x),
                    catmullrom(d, p0.y, p1.y, p2.y, p3.y),
                    catmullrom(d, p0.z, p1.z, p2.z, p3.z),
                )
            }
            Self::Bezier => {
                let prev = &frames[current];
                let next = &frames[target];
                let p0 = prev.target_b();
                let p3 = next.target_a();
                let p1 = prev.bezier_right.add(p0);
                let p2 = next.bezier_left.add(p3);
                let p1t = prev.bezier_right_time;
                let p2t = next.bezier_left_time;
                let d = delta as f64;
                Vec3::new(
                    bezier(
                        bezier_find_t(d, 0.0, p1t.x, p2t.x, 1.0),
                        p0.x,
                        p1.x,
                        p2.x,
                        p3.x,
                    ),
                    bezier(
                        bezier_find_t(d, 0.0, p1t.y, p2t.y, 1.0),
                        p0.y,
                        p1.y,
                        p2.y,
                        p3.y,
                    ),
                    bezier(
                        bezier_find_t(d, 0.0, p1t.z, p2t.z, 1.0),
                        p0.z,
                        p1.z,
                        p2.z,
                        p3.z,
                    ),
                )
            }
            Self::Step => frames[current].target_b(),
        };
        apply_strength(result, strength, ty)
    }
}

fn apply_strength(result: Vec3, strength: f32, ty: TransformType) -> Vec3 {
    let s = strength as f64;
    if ty == TransformType::Scale {
        result.offset(-1.0).scale(s).offset(1.0)
    } else {
        result.scale(s)
    }
}
