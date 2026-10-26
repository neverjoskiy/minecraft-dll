

use crate::avatar::math::Vec3;
use crate::avatar::model::{ModelPart, ParentType};

#[derive(Clone, Copy, Debug)]
pub struct VanillaPartData {
    pub pos: Vec3,
    pub rot: Vec3,
    pub scale: Vec3,
    pub visible: bool,
}

impl Default for VanillaPartData {
    fn default() -> Self {
        Self {
            pos: Vec3::zero(),
            rot: Vec3::zero(),
            scale: Vec3::one(),
            visible: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VanillaPose {
    pub head: VanillaPartData,
    pub body: VanillaPartData,
    pub left_arm: VanillaPartData,
    pub right_arm: VanillaPartData,
    pub left_leg: VanillaPartData,
    pub right_leg: VanillaPartData,
}

impl VanillaPose {
    
    
    pub fn from_model_part(
        x: f32,
        y: f32,
        z: f32,
        x_rot: f32,
        y_rot: f32,
        z_rot: f32,
        x_scale: f32,
        y_scale: f32,
        z_scale: f32,
        visible: bool,
    ) -> VanillaPartData {
        VanillaPartData {
            pos: Vec3::new(x as f64, y as f64, (-z) as f64),
            rot: Vec3::new(
                (-x_rot).to_degrees() as f64,
                (-y_rot).to_degrees() as f64,
                z_rot.to_degrees() as f64,
            ),
            scale: Vec3::new(x_scale as f64, y_scale as f64, z_scale as f64),
            visible,
        }
    }

    
    pub fn from_render_state(
        head_pitch_deg: f32,
        head_yaw_deg: f32,
        walk_pos: f32,
        walk_speed: f32,
        speed_value: f32,
        crouching: bool,
        age_in_ticks: f32,
    ) -> Self {
        let speed = if speed_value.abs() < 1e-3 {
            1.0
        } else {
            speed_value
        };
        let arm = (walk_pos * 0.6662).cos() * 2.0 * walk_speed * 0.5 / speed;
        let arm_opp =
            (walk_pos * 0.6662 + std::f32::consts::PI).cos() * 2.0 * walk_speed * 0.5 / speed;
        let leg = (walk_pos * 0.6662).cos() * 1.4 * walk_speed / speed;
        let leg_opp = (walk_pos * 0.6662 + std::f32::consts::PI).cos() * 1.4 * walk_speed / speed;
        let bob = (age_in_ticks * 0.09).cos() * 0.05 + 0.05;

        let mut pose = Self {
            head: VanillaPartData {
                rot: Vec3::new((-head_pitch_deg) as f64, (-head_yaw_deg) as f64, 0.0),
                scale: Vec3::one(),
                visible: true,
                ..Default::default()
            },
            body: VanillaPartData {
                scale: Vec3::one(),
                visible: true,
                ..Default::default()
            },
            left_arm: VanillaPartData {
                
                pos: ParentType::LeftArm.offset(),
                rot: Vec3::new((-arm.to_degrees()) as f64, 0.0, (-bob).to_degrees() as f64),
                scale: Vec3::one(),
                visible: true,
            },
            right_arm: VanillaPartData {
                pos: ParentType::RightArm.offset(),
                rot: Vec3::new((-arm_opp.to_degrees()) as f64, 0.0, bob.to_degrees() as f64),
                scale: Vec3::one(),
                visible: true,
            },
            left_leg: VanillaPartData {
                pos: ParentType::LeftLeg.offset(),
                rot: Vec3::new((-leg_opp.to_degrees()) as f64, 0.0, 0.0),
                scale: Vec3::one(),
                visible: true,
            },
            right_leg: VanillaPartData {
                pos: ParentType::RightLeg.offset(),
                rot: Vec3::new((-leg.to_degrees()) as f64, 0.0, 0.0),
                scale: Vec3::one(),
                visible: true,
            },
        };
        if crouching {
            pose.body.rot.x = 0.5_f32.to_degrees() as f64;
            pose.right_arm.rot.x += (-0.62831855_f32).to_degrees() as f64;
            pose.left_arm.rot.x += (-0.62831855_f32).to_degrees() as f64;
            
            pose.left_arm.pos.y += 3.2;
            pose.right_arm.pos.y += 3.2;
            pose.head.pos.y += 4.2;
            pose.body.pos.y += 3.2;
            pose.left_leg.pos.z += 4.0;
            pose.right_leg.pos.z += 4.0;
        }
        pose
    }

    pub fn magnitude(&self) -> f32 {
        (self.head.rot.x.abs()
            + self.head.rot.y.abs()
            + self.left_arm.rot.x.abs()
            + self.right_arm.rot.x.abs()
            + self.left_leg.rot.x.abs()
            + self.right_leg.rot.x.abs()) as f32
    }

    
    pub fn bake_fingerprint(&self) -> u64 {
        fn mix(h: u64, f: f64) -> u64 {
            let q = (f * 256.0).round() as i32 as u32;
            h.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(q as u64)
        }
        fn part(mut h: u64, p: &VanillaPartData) -> u64 {
            h = mix(h, p.pos.x);
            h = mix(h, p.pos.y);
            h = mix(h, p.pos.z);
            h = mix(h, p.rot.x);
            h = mix(h, p.rot.y);
            h = mix(h, p.rot.z);
            h = mix(h, p.scale.x);
            h = mix(h, p.scale.y);
            h = mix(h, p.scale.z);
            h ^ if p.visible { 1 } else { 0 }
        }
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        h = part(h, &self.head);
        h = part(h, &self.body);
        h = part(h, &self.left_arm);
        h = part(h, &self.right_arm);
        h = part(h, &self.left_leg);
        h = part(h, &self.right_leg);
        h
    }

    fn data_for(&self, parent: ParentType) -> Option<VanillaPartData> {
        Some(match parent {
            ParentType::Head => self.head,
            ParentType::Body => self.body,
            ParentType::LeftArm => self.left_arm,
            ParentType::RightArm => self.right_arm,
            ParentType::LeftLeg => self.left_leg,
            ParentType::RightLeg => self.right_leg,
            _ => return None,
        })
    }
}







pub fn apply_vanilla_pose(root: &mut ModelPart, pose: &VanillaPose) {
    apply_vanilla_rec(root, pose, ParentType::None);
}

fn is_pose_limb(parent: ParentType) -> bool {
    matches!(
        parent,
        ParentType::Head
            | ParentType::Body
            | ParentType::LeftArm
            | ParentType::RightArm
            | ParentType::LeftLeg
            | ParentType::RightLeg
    )
}

fn apply_vanilla_rec(part: &mut ModelPart, pose: &VanillaPose, ancestor_limb: ParentType) {
    let resolved = if part.parent_type != ParentType::None {
        part.parent_type
    } else {
        ParentType::from_name(&part.name)
    };

    let apply_here =
        !part.children.is_empty() && is_pose_limb(resolved) && ancestor_limb == ParentType::None;

    if apply_here {
        if let Some(data) = pose.data_for(resolved) {
            let mut default_pivot = resolved.offset().sub(data.pos);

            if part.animation_override & 4 == 0 {
                let new_scale = data.scale.mul(part.vanilla.scale);
                default_pivot = default_pivot.mul(new_scale);
                part.vanilla.scale = new_scale;
            }
            if part.animation_override & 2 == 0 {
                part.vanilla.pivot = part.vanilla.pivot.add(default_pivot);
                part.vanilla.pos = part.vanilla.pos.add(default_pivot);
            }
            if part.animation_override & 1 == 0 {
                part.vanilla.rotation = part.vanilla.rotation.add(data.rot);
            }
            part.animated = true;
        }
    }

    let child_ancestor = if is_pose_limb(resolved) {
        
        if ancestor_limb == ParentType::None {
            resolved
        } else {
            ancestor_limb
        }
    } else {
        ancestor_limb
    };

    for child in &mut part.children {
        apply_vanilla_rec(child, pose, child_ancestor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::math::Vec3;
    use crate::avatar::model::ModelPart;

    #[test]
    fn nested_body_under_leg_does_not_get_body_pose() {
        let mut root = ModelPart::new("root");
        let mut leg = ModelPart::new_group("RightLeg");
        leg.origin = Vec3::new(-1.9, 12.0, 0.0);
        let mut nested_body = ModelPart::new_group("Body");
        nested_body.children.push(ModelPart::new_leaf("cube"));
        leg.children.push(nested_body);
        root.children.push(leg);

        let mut pose = VanillaPose::default();
        pose.body.rot = Vec3::new(15.0, 0.0, 0.0);
        pose.right_leg.rot = Vec3::new(30.0, 0.0, 0.0);

        apply_vanilla_pose(&mut root, &pose);

        assert!(
            (root.children[0].vanilla.rotation.x - 30.0).abs() < 1e-6,
            "leg should swing"
        );
        assert!(
            root.children[0].children[0].vanilla.rotation.x.abs() < 1e-6,
            "nested Body must not get torso sway"
        );
    }

    #[test]
    fn leaf_cube_does_not_get_vanilla_even_if_named_head() {
        
        let mut root = ModelPart::new("root");
        let mut head = ModelPart::new_group("Head");
        head.children.push(ModelPart::new_leaf("Head"));
        root.children.push(head);

        let mut pose = VanillaPose::default();
        pose.head.rot = Vec3::new(0.0, 45.0, 0.0);
        apply_vanilla_pose(&mut root, &pose);

        assert!((root.children[0].vanilla.rotation.y - 45.0).abs() < 1e-6);
        assert!(root.children[0].children[0].vanilla.rotation.y.abs() < 1e-6);
    }
}
