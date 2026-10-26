use crate::avatar::math::Vec3;
use crate::avatar::model::ModelPart;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformType {
    Position,
    Rotation,
    GlobalRot,
    Scale,
}

impl TransformType {
    pub fn from_channel(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "position" | "pos" => Some(Self::Position),
            "rotation" | "rot" => Some(Self::Rotation),
            "global_rot" | "global_rotation" => Some(Self::GlobalRot),
            "scale" => Some(Self::Scale),
            _ => None,
        }
    }

    pub fn apply(self, part: &mut ModelPart, vec: Vec3, merge: bool) {
        match self {
            Self::Position => part.anim_position(vec, merge),
            Self::Rotation => part.anim_rotation(vec, merge),
            Self::GlobalRot => part.global_anim_rot(vec, merge),
            Self::Scale => part.anim_scale(vec, merge),
        }
    }
}
