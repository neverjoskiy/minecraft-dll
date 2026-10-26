

use std::collections::{HashMap, HashSet};

use crate::avatar::format::{BbElement, BbGroup, BbGroupChild, BbModel};
use crate::avatar::math::{Mat3, Mat4, Vec3};
use crate::avatar::model::{ModelPart, PartGeometry};
use crate::avatar::render::mesh::{self, TexturedMesh};


pub fn build_model_tree(model: &BbModel) -> ModelPart {
    let mut by_uuid: HashMap<String, BbElement> = HashMap::new();
    let mut by_name: HashMap<String, BbElement> = HashMap::new();
    for element in &model.elements {
        if let Some(uuid) = &element.uuid {
            by_uuid.insert(uuid.clone(), element.clone());
        }
        by_name.insert(element.name.clone(), element.clone());
    }

    let mut root = ModelPart::new(model.name.clone());
    let mut consumed: HashSet<String> = HashSet::new();

    if model.groups.is_empty() {
        for element in &model.elements {
            root.children.push(element_part(element, &model.textures));
        }
        return root;
    }

    for group in &model.groups {
        root.children.push(group_to_part(
            group,
            &mut by_uuid,
            &mut by_name,
            &mut consumed,
            &model.textures,
        ));
    }

    for element in &model.elements {
        let id = element.uuid.clone().unwrap_or_else(|| element.name.clone());
        if consumed.contains(&id) || consumed.contains(&element.name) {
            continue;
        }
        root.children.push(element_part(element, &model.textures));
    }
    root
}

fn element_part(element: &BbElement, textures: &[crate::avatar::format::TextureRef]) -> ModelPart {
    
    
    let mut part = ModelPart::new_leaf(element.name.clone());
    let meshes = mesh::meshes_from_elements(std::slice::from_ref(element), textures);
    part.geometry = if meshes.is_empty() {
        PartGeometry::Element(element.clone())
    } else {
        PartGeometry::Meshes(meshes)
    };
    part
}

fn group_to_part(
    group: &BbGroup,
    by_uuid: &mut HashMap<String, BbElement>,
    by_name: &mut HashMap<String, BbElement>,
    consumed: &mut HashSet<String>,
    textures: &[crate::avatar::format::TextureRef],
) -> ModelPart {
    let mut part = ModelPart::new_group(group.name.clone());
    part.origin = Vec3::new(
        group.origin.x as f64,
        group.origin.y as f64,
        group.origin.z as f64,
    );
    
    if let Some([rx, ry, rz]) = group.rotation {
        part.rotation = Vec3::new(rx as f64, ry as f64, rz as f64);
    }
    for child in &group.children {
        match child {
            BbGroupChild::Group(g) => {
                part.children
                    .push(group_to_part(g, by_uuid, by_name, consumed, textures));
            }
            BbGroupChild::Element(key) => {
                let element = by_uuid.remove(key).or_else(|| by_name.remove(key));
                if let Some(element) = element {
                    if let Some(uuid) = &element.uuid {
                        consumed.insert(uuid.clone());
                        by_uuid.remove(uuid);
                    }
                    consumed.insert(element.name.clone());
                    by_name.remove(&element.name);
                    part.children.push(element_part(&element, textures));
                }
            }
        }
    }
    part
}






pub fn flatten_textured_meshes(root: &ModelPart) -> Vec<TexturedMesh> {
    let mut buckets: Vec<(Option<u32>, mesh::Mesh)> = Vec::new();
    flatten_textured_meshes_into(root, &mut buckets);
    buckets
        .into_iter()
        .filter(|(_, m)| !m.vertices.is_empty())
        .map(|(texture_index, mesh)| TexturedMesh {
            texture_index,
            mesh,
        })
        .collect()
}


pub fn flatten_textured_meshes_into(
    root: &ModelPart,
    buckets: &mut Vec<(Option<u32>, mesh::Mesh)>,
) {
    for (_, m) in buckets.iter_mut() {
        m.vertices.clear();
    }
    flatten_rec(root, Mat4::identity(), buckets);
    buckets.retain(|(_, m)| !m.vertices.is_empty());
}




pub fn flatten_into_mesh_map(
    root: &ModelPart,
    handles: &mut Vec<(Option<u32>, i64)>,
    meshes: &mut HashMap<u64, mesh::Mesh>,
    mut alloc_handle: impl FnMut() -> u64,
) {
    for (_, h) in handles.iter() {
        if let Some(m) = meshes.get_mut(&(*h as u64)) {
            m.vertices.clear();
        }
    }
    flatten_rec_map(root, handles, meshes, &mut alloc_handle);
    handles.retain(|(_, h)| {
        meshes
            .get(&(*h as u64))
            .is_some_and(|m| !m.vertices.is_empty())
    });
}


pub fn recompute_world_matrices(root: &mut ModelPart) {
    recompute_world_rec(root, Mat4::identity());
}

fn recompute_world_rec(part: &mut ModelPart, parent: Mat4) {
    if !part.visible || part.parent_type.is_separate() {
        part.world = parent;
        return;
    }
    let local = local_matrix(part);
    part.world = if local.is_identity() {
        parent
    } else if parent.is_identity() {
        local
    } else {
        parent.mul(local)
    };
    let world = part.world;
    for child in &mut part.children {
        recompute_world_rec(child, world);
    }
}


pub fn find_part_by_parent_type<'a>(
    root: &'a ModelPart,
    target: crate::avatar::model::ParentType,
) -> Option<&'a ModelPart> {
    if root.parent_type == target {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_part_by_parent_type(child, target) {
            return Some(found);
        }
    }
    None
}

fn flatten_rec(part: &ModelPart, parent: Mat4, buckets: &mut Vec<(Option<u32>, mesh::Mesh)>) {
    if !part.visible {
        return;
    }
    
    if part.parent_type.is_separate() {
        return;
    }

    let world = parent.mul(local_matrix(part));

    if !part.parent_type.is_pivot() {
        if let PartGeometry::Meshes(meshes) = &part.geometry {
            for textured in meshes {
                let idx = if let Some(i) = buckets
                    .iter()
                    .position(|(t, _)| *t == textured.texture_index)
                {
                    i
                } else {
                    buckets.push((textured.texture_index, mesh::Mesh::default()));
                    buckets.len() - 1
                };
                push_transformed_verts(&mut buckets[idx].1, &textured.mesh, world);
            }
        }
    }

    for child in &part.children {
        flatten_rec(child, world, buckets);
    }
}

fn flatten_rec_map(
    part: &ModelPart,
    handles: &mut Vec<(Option<u32>, i64)>,
    meshes: &mut HashMap<u64, mesh::Mesh>,
    alloc_handle: &mut dyn FnMut() -> u64,
) {
    if !part.visible {
        return;
    }
    if part.parent_type.is_separate() {
        return;
    }

    let world = part.world;

    if !part.parent_type.is_pivot() {
        if let PartGeometry::Meshes(part_meshes) = &part.geometry {
            for textured in part_meshes {
                let handle = if let Some((_, h)) =
                    handles.iter().find(|(t, _)| *t == textured.texture_index)
                {
                    *h
                } else {
                    let h = alloc_handle() as i64;
                    handles.push((textured.texture_index, h));
                    meshes.entry(h as u64).or_default();
                    h
                };
                let bucket = meshes.entry(handle as u64).or_default();
                push_transformed_verts(bucket, &textured.mesh, world);
            }
        }
    }

    for child in &part.children {
        flatten_rec_map(child, handles, meshes, alloc_handle);
    }
}

fn push_transformed_verts(dst: &mut mesh::Mesh, src: &mesh::Mesh, world: Mat4) {
    dst.vertices.reserve(src.vertices.len());
    if world.is_identity() {
        dst.vertices.extend_from_slice(&src.vertices);
        return;
    }
    for v in &src.vertices {
        let (px, py, pz) = world.transform_xyz(v.x as f64, v.y as f64, v.z as f64);
        dst.vertices.push(mesh::MeshVertex {
            x: px as f32,
            y: py as f32,
            z: pz as f32,
            u: v.u,
            v: v.v,
            nx: v.nx,
            ny: v.ny,
            nz: v.nz,
            r: v.r,
            g: v.g,
            b: v.b,
            a: v.a,
        });
    }
}

fn local_matrix(part: &ModelPart) -> Mat4 {
    
    
    
    
    
    let pivot = Vec3::new(
        part.origin.x / 16.0,
        part.origin.y / 16.0,
        part.origin.z / 16.0,
    );
    let offset_pivot = Vec3::new(
        part.vanilla.pivot.x / 16.0,
        part.vanilla.pivot.y / 16.0,
        part.vanilla.pivot.z / 16.0,
    );
    let offset_pos = Vec3::new(
        part.vanilla.pos.x / 16.0,
        part.vanilla.pos.y / 16.0,
        part.vanilla.pos.z / 16.0,
    );
    let pos = Vec3::new(
        (part.position.x + part.anim.position.x) / 16.0,
        (part.position.y + part.anim.position.y) / 16.0,
        (part.position.z + part.anim.position.z) / 16.0,
    );
    let rot = part
        .rotation
        .add(part.vanilla.rotation)
        .add(part.anim.rotation)
        .add(part.anim.global_rot);
    let scale = part.vanilla.scale.mul(part.scale).mul(part.anim.scale);

    let no_rot = rot.x.abs() < 1e-9 && rot.y.abs() < 1e-9 && rot.z.abs() < 1e-9;
    let no_pos = pos.x.abs() < 1e-9
        && pos.y.abs() < 1e-9
        && pos.z.abs() < 1e-9
        && offset_pos.x.abs() < 1e-9
        && offset_pos.y.abs() < 1e-9
        && offset_pos.z.abs() < 1e-9
        && offset_pivot.x.abs() < 1e-9
        && offset_pivot.y.abs() < 1e-9
        && offset_pivot.z.abs() < 1e-9
        && pivot.x.abs() < 1e-9
        && pivot.y.abs() < 1e-9
        && pivot.z.abs() < 1e-9;
    let no_scale = (scale.x - 1.0).abs() < 1e-9
        && (scale.y - 1.0).abs() < 1e-9
        && (scale.z - 1.0).abs() < 1e-9;
    if part.children.is_empty() && no_rot && no_pos && no_scale {
        return Mat4::identity();
    }
    if no_rot && no_pos && no_scale {
        return Mat4::identity();
    }

    let t1 = Mat4::translation(Vec3::new(
        offset_pos.x - pivot.x - offset_pivot.x,
        offset_pos.y - pivot.y - offset_pivot.y,
        offset_pos.z - pivot.z - offset_pivot.z,
    ));
    let t2 = Mat4::translation(Vec3::new(
        pos.x + pivot.x + offset_pivot.x,
        pos.y + pivot.y + offset_pivot.y,
        pos.z + pivot.z + offset_pivot.z,
    ));
    if no_rot && no_scale {
        
        return Mat4::translation(Vec3::new(
            t1.m[3][0] + t2.m[3][0],
            t1.m[3][1] + t2.m[3][1],
            t1.m[3][2] + t2.m[3][2],
        ));
    }
    let mut m = t2;
    if !no_rot {
        m.mul_assign(Mat4::from_rotation(Mat3::from_euler_zyx_deg(
            rot.x, rot.y, rot.z,
        )));
    }
    if !no_scale {
        m.mul_assign(Mat4::scale(scale));
    }
    m.mul_assign(t1);
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::math::Vec3;
    use crate::avatar::model::{ParentType, VanillaPose};

    #[test]
    fn pivot_identity_does_not_move_points() {
        let mut part = ModelPart::new_group("LeftArm");
        part.origin = Vec3::new(-5.0, 22.0, 0.0);
        let m = local_matrix(&part);
        let p = m.transform_point(Vec3::new(-0.25, 1.25, 0.0));
        assert!((p.x + 0.25).abs() < 1e-6, "x {}", p.x);
        assert!((p.y - 1.25).abs() < 1e-6, "y {}", p.y);
        assert!(p.z.abs() < 1e-6, "z {}", p.z);
    }

    #[test]
    fn leaf_named_left_arm_is_not_parent_type() {
        let leaf = ModelPart::new_leaf("LeftArm");
        assert_eq!(leaf.parent_type, ParentType::None);
    }

    #[test]
    fn full_body_height_is_one_block_per_16_units() {
        
        
        
        let raw = r#"{
            "name": "t",
            "elements": [{
                "name": "block",
                "uuid": "a1",
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "origin": [0, 0, 0],
                "faces": { "north": { "uv": [0,0,16,16], "texture": 0 } }
            }],
            "outliner": ["a1"],
            "textures": [{ "name": "skin" }]
        }"#;
        let model = crate::avatar::format::BbModel::from_json_str(raw).expect("parse");
        let tree = build_model_tree(&model);
        let meshes = flatten_textured_meshes(&tree);
        let min_y = meshes
            .iter()
            .flat_map(|t| t.mesh.vertices.iter().map(|v| v.y))
            .fold(f32::INFINITY, f32::min);
        let max_y = meshes
            .iter()
            .flat_map(|t| t.mesh.vertices.iter().map(|v| v.y))
            .fold(f32::NEG_INFINITY, f32::max);
        let height = max_y - min_y;
        assert!(
            (height - 1.0).abs() < 1e-4,
            "16-unit cube must bake to 1 block tall, got {height}"
        );
    }

    #[test]
    fn vanilla_arm_swing_moves_mesh_verts() {
        let raw = r#"{
            "name": "t",
            "elements": [{
                "name": "arm",
                "uuid": "a1",
                "from": [-6, 12, -2],
                "to": [-4, 22, 2],
                "origin": [-5, 22, 0],
                "faces": { "north": { "uv": [0,0,2,10], "texture": 0 } }
            }],
            "outliner": [{
                "name": "LeftArm",
                "uuid": "g1",
                "origin": [-5, 22, 0],
                "children": ["a1"]
            }],
            "textures": [{ "name": "skin" }]
        }"#;
        let model = crate::avatar::format::BbModel::from_json_str(raw).expect("parse");
        let mut tree = build_model_tree(&model);
        let rest = flatten_textured_meshes(&tree);
        let rest_y: f32 = rest
            .iter()
            .flat_map(|t| t.mesh.vertices.iter().map(|v| v.y))
            .fold(f32::NEG_INFINITY, f32::max);

        let pose = VanillaPose::from_render_state(0.0, 0.0, 1.0, 1.0, 1.0, false, 0.0);
        tree.reset_animation();
        crate::avatar::model::apply_vanilla_pose(&mut tree, &pose);
        let swung = flatten_textured_meshes(&tree);
        let swung_y: f32 = swung
            .iter()
            .flat_map(|t| t.mesh.vertices.iter().map(|v| v.y))
            .fold(f32::NEG_INFINITY, f32::max);

        assert!(
            (rest_y - swung_y).abs() > 0.01,
            "arm swing should move verts rest_y={rest_y} swung_y={swung_y} la={}",
            pose.left_arm.rot.x
        );
    }
}
