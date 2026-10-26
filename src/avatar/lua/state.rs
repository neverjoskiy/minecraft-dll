

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::avatar::animation::{Animation, animation_from_bb, uuid_name_map};
use crate::avatar::format::AvatarPackage;
use crate::avatar::model::{ModelPart, build_model_tree};

#[derive(Debug)]
pub struct ScriptWorld {
    
    pub models: ModelPart,
    
    pub animations: HashMap<String, HashMap<String, Animation>>,
    
    pub vanilla_player_visible: Option<bool>,
}

pub type SharedWorld = Arc<Mutex<ScriptWorld>>;

impl ScriptWorld {
    pub fn from_package(package: &AvatarPackage) -> Self {
        let mut models = ModelPart::new_leaf("models");
        let mut animations: HashMap<String, HashMap<String, Animation>> = HashMap::new();

        for model in &package.models {
            let stem = model
                .file_stem
                .clone()
                .unwrap_or_else(|| model.name.clone());
            let mut tree = build_model_tree(model);
            
            
            strip_part_geometry(&mut tree);
            
            tree.name = stem.clone();
            tree.parent_type = crate::avatar::model::ParentType::None;
            models.children.push(tree);

            let names = uuid_name_map(model);
            let mut by_name = HashMap::new();
            for def in &model.animations {
                let anim = animation_from_bb(&stem, def, &names);
                
                
                
                by_name.insert(def.name.clone(), anim);
            }
            if !by_name.is_empty() {
                animations.insert(stem, by_name);
            }
        }

        Self {
            models,
            animations,
            vanilla_player_visible: None,
        }
    }

    pub fn share(self) -> SharedWorld {
        Arc::new(Mutex::new(self))
    }
}

fn strip_part_geometry(part: &mut ModelPart) {
    part.geometry = crate::avatar::model::PartGeometry::Empty;
    for child in &mut part.children {
        strip_part_geometry(child);
    }
}
