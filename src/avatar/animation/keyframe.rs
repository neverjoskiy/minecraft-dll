use crate::avatar::animation::Interpolation;
use crate::avatar::math::Vec3;

#[derive(Clone, Debug)]
pub struct Keyframe {
    pub time: f32,
    pub interpolation: Interpolation,
    pub target_a: Vec3,
    pub target_b: Vec3,
    pub bezier_left: Vec3,
    pub bezier_right: Vec3,
    pub bezier_left_time: Vec3,
    pub bezier_right_time: Vec3,
}

impl Keyframe {
    pub fn new(time: f32, value: Vec3, interpolation: Interpolation) -> Self {
        Self {
            time,
            interpolation,
            target_a: value,
            target_b: value,
            bezier_left: Vec3::zero(),
            bezier_right: Vec3::zero(),
            bezier_left_time: Vec3::new(0.0, 0.0, 0.0),
            bezier_right_time: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    pub fn target_a(&self) -> Vec3 {
        self.target_a
    }

    pub fn target_b(&self) -> Vec3 {
        self.target_b
    }
}
