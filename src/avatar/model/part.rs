

use crate::avatar::format::BbElement;
use crate::avatar::math::{Mat4, Vec3};
use crate::avatar::model::ParentType;
use crate::avatar::render::mesh::TexturedMesh;


#[derive(Clone, Debug, Default)]
pub struct AnimPose {
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
    pub global_rot: Vec3,
}

impl AnimPose {
    pub fn identity_scale() -> Self {
        Self {
            position: Vec3::zero(),
            rotation: Vec3::zero(),
            scale: Vec3::one(),
            global_rot: Vec3::zero(),
        }
    }
}



#[derive(Clone, Debug, Default)]
pub struct VanillaOffset {
    pub pos: Vec3,
    pub pivot: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl VanillaOffset {
    pub fn identity() -> Self {
        Self {
            pos: Vec3::zero(),
            pivot: Vec3::zero(),
            rotation: Vec3::zero(),
            scale: Vec3::one(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PartGeometry {
    Empty,
    Meshes(Vec<TexturedMesh>),
    Element(BbElement),
}

#[derive(Clone, Debug)]
pub struct ModelPart {
    pub name: String,
    pub parent_type: ParentType,
    
    pub origin: Vec3,
    
    pub position: Vec3,
    
    pub rotation: Vec3,
    
    pub scale: Vec3,
    pub visible: bool,
    pub geometry: PartGeometry,
    pub children: Vec<ModelPart>,
    pub anim: AnimPose,
    pub vanilla: VanillaOffset,
    pub last_animation_priority: i32,
    pub animation_override: u8,
    pub animated: bool,
    
    pub world: Mat4,
}

impl ModelPart {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let parent_type = ParentType::from_name(&name);
        Self::with_parent(name, parent_type)
    }

    pub fn new_group(name: impl Into<String>) -> Self {
        let name = name.into();
        Self::with_parent(name.clone(), ParentType::from_name(&name))
    }

    pub fn new_leaf(name: impl Into<String>) -> Self {
        Self::with_parent(name.into(), ParentType::None)
    }

    fn with_parent(name: String, parent_type: ParentType) -> Self {
        Self {
            name,
            parent_type,
            origin: Vec3::zero(),
            position: Vec3::zero(),
            rotation: Vec3::zero(),
            scale: Vec3::one(),
            visible: true,
            geometry: PartGeometry::Empty,
            children: Vec::new(),
            anim: AnimPose::identity_scale(),
            vanilla: VanillaOffset::identity(),
            last_animation_priority: i32::MIN,
            animation_override: 0,
            animated: false,
            world: Mat4::identity(),
        }
    }

    
    pub fn reset_animation(&mut self) {
        self.anim = AnimPose::identity_scale();
        self.vanilla = VanillaOffset::identity();
        self.last_animation_priority = i32::MIN;
        self.animation_override = 0;
        self.animated = false;
        for child in &mut self.children {
            child.reset_animation();
        }
    }

    
    pub fn anim_position(&mut self, v: Vec3, merge: bool) {
        let flipped = Vec3::new(-v.x, v.y, v.z);
        if merge {
            self.anim.position = self.anim.position.add(flipped);
        } else {
            self.anim.position = flipped;
        }
    }

    
    pub fn anim_rotation(&mut self, v: Vec3, merge: bool) {
        let flipped = Vec3::new(-v.x, -v.y, v.z);
        if merge {
            self.anim.rotation = self.anim.rotation.add(flipped);
        } else {
            self.anim.rotation = flipped;
        }
    }

    pub fn global_anim_rot(&mut self, v: Vec3, merge: bool) {
        self.anim_rotation(v, merge);
    }

    pub fn anim_scale(&mut self, v: Vec3, merge: bool) {
        if merge {
            self.anim.scale = self.anim.scale.mul(v);
        } else {
            self.anim.scale = v;
        }
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut ModelPart> {
        if self.name == name {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(name) {
                return Some(found);
            }
        }
        None
    }

    pub fn walk_mut<F: FnMut(&mut ModelPart)>(&mut self, f: &mut F) {
        f(self);
        for child in &mut self.children {
            child.walk_mut(f);
        }
    }
}
