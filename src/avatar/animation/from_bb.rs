

use std::collections::HashMap;

use crate::avatar::animation::{
    Animation, AnimationChannel, Interpolation, Keyframe, LoopMode, PartChannels, TransformType,
};
use crate::avatar::format::{AnimationDef, BbGroup, BbGroupChild, BbModel};
use crate::avatar::math::Vec3;
use serde_json::Value;


pub fn uuid_name_map(model: &BbModel) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for element in &model.elements {
        if let Some(uuid) = &element.uuid {
            map.insert(uuid.clone(), element.name.clone());
        }
        map.insert(element.name.clone(), element.name.clone());
    }
    walk_groups(&model.groups, &mut map);
    map
}

fn walk_groups(groups: &[BbGroup], map: &mut HashMap<String, String>) {
    for group in groups {
        if let Some(uuid) = &group.uuid {
            map.insert(uuid.clone(), group.name.clone());
        }
        map.insert(group.name.clone(), group.name.clone());
        for child in &group.children {
            if let BbGroupChild::Group(nested) = child {
                walk_groups(std::slice::from_ref(nested), map);
            }
        }
    }
}


pub fn animation_from_bb(
    model_name: &str,
    def: &AnimationDef,
    uuid_names: &HashMap<String, String>,
) -> Animation {
    let mut anim = Animation::new(model_name, def.name.clone());
    anim.length = def.length;
    anim.loop_mode = LoopMode::from_bb(def.loop_mode.as_deref());
    if def.override_vanilla {
        anim.override_mask = 7;
    }

    for animator in &def.animators {
        if animator.id.eq_ignore_ascii_case("effects") {
            continue;
        }
        let part_name = uuid_names
            .get(&animator.id)
            .cloned()
            .unwrap_or_else(|| animator.id.clone());

        let mut by_channel: Vec<(TransformType, Vec<Keyframe>)> = Vec::new();
        for stub in &animator.keyframes {
            let channel =
                if animator.rotation_global && stub.channel.eq_ignore_ascii_case("rotation") {
                    "global_rot"
                } else {
                    stub.channel.as_str()
                };
            let Some(ty) = TransformType::from_channel(channel) else {
                continue;
            };
            let value = parse_vec3(&stub.data).unwrap_or_else(|| {
                if ty == TransformType::Scale {
                    Vec3::one()
                } else {
                    Vec3::zero()
                }
            });
            let interp = Interpolation::from_name(&stub.interpolation);
            let kf = Keyframe::new(stub.time, value, interp);
            if let Some((_, frames)) = by_channel.iter_mut().find(|(t, _)| *t == ty) {
                frames.push(kf);
            } else {
                by_channel.push((ty, vec![kf]));
            }
        }
        for (_, frames) in &mut by_channel {
            frames.sort_by(|a, b| {
                a.time
                    .partial_cmp(&b.time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        if !by_channel.is_empty() {
            anim.parts.push(PartChannels {
                part_name,
                channels: by_channel
                    .into_iter()
                    .map(|(transform, keyframes)| AnimationChannel {
                        transform,
                        keyframes,
                    })
                    .collect(),
            });
        }
    }

    
    if anim.parts.is_empty() && !def.keyframes.is_empty() {
        let mut by_channel: Vec<(TransformType, Vec<Keyframe>)> = Vec::new();
        for stub in &def.keyframes {
            let Some(ty) = TransformType::from_channel(&stub.channel) else {
                continue;
            };
            let value = parse_vec3(&stub.data).unwrap_or(Vec3::zero());
            let kf = Keyframe::new(stub.time, value, Interpolation::Linear);
            if let Some((_, frames)) = by_channel.iter_mut().find(|(t, _)| *t == ty) {
                frames.push(kf);
            } else {
                by_channel.push((ty, vec![kf]));
            }
        }
        for (_, frames) in &mut by_channel {
            frames.sort_by(|a, b| {
                a.time
                    .partial_cmp(&b.time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        if !by_channel.is_empty() {
            anim.parts.push(PartChannels {
                part_name: model_name.to_string(),
                channels: by_channel
                    .into_iter()
                    .map(|(transform, keyframes)| AnimationChannel {
                        transform,
                        keyframes,
                    })
                    .collect(),
            });
        }
    }

    
    if anim.length <= 0.0 {
        let mut max_t = 0.0f32;
        for part in &anim.parts {
            for ch in &part.channels {
                for kf in &ch.keyframes {
                    max_t = max_t.max(kf.time);
                }
            }
        }
        if max_t > 0.0 {
            anim.length = max_t;
        }
    }

    anim
}

fn parse_vec3(value: &Value) -> Option<Vec3> {
    if let Some(arr) = value.as_array() {
        if arr.len() >= 3 {
            return Some(Vec3::new(
                parse_num(&arr[0]).unwrap_or(0.0),
                parse_num(&arr[1]).unwrap_or(0.0),
                parse_num(&arr[2]).unwrap_or(0.0),
            ));
        }
    }
    if let Some(obj) = value.as_object() {
        if let Some(arr) = obj.get("pos").or_else(|| obj.get("position")) {
            return parse_vec3(arr);
        }
        let x = obj.get("x").and_then(parse_num).unwrap_or(0.0);
        let y = obj.get("y").and_then(parse_num).unwrap_or(0.0);
        let z = obj.get("z").and_then(parse_num).unwrap_or(0.0);
        return Some(Vec3::new(x, y, z));
    }
    None
}

fn parse_num(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|v| v as f64))
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::format::BbModel;

    #[test]
    fn parses_blockbench_animators_by_uuid() {
        let raw = r#"{
            "name": "m",
            "elements": [{
                "name": "cube",
                "uuid": "e1",
                "from": [0,0,0], "to": [1,1,1],
                "faces": { "north": { "uv": [0,0,1,1], "texture": 0 } }
            }],
            "outliner": [{
                "name": "Head",
                "uuid": "11111111-1111-1111-1111-111111111111",
                "origin": [0, 24, 0],
                "children": ["e1"]
            }],
            "textures": [{ "name": "t" }],
            "animations": [{
                "name": "idle",
                "loop": "loop",
                "length": 1.0,
                "animators": {
                    "11111111-1111-1111-1111-111111111111": {
                        "keyframes": [
                            {
                                "channel": "rotation",
                                "interpolation": "linear",
                                "time": 0.0,
                                "data_points": [{ "x": 0, "y": 0, "z": 0 }]
                            },
                            {
                                "channel": "rotation",
                                "interpolation": "linear",
                                "time": 1.0,
                                "data_points": [{ "x": 45, "y": 0, "z": 0 }]
                            }
                        ]
                    }
                }
            }]
        }"#;
        let model = BbModel::from_json_str(raw).expect("parse");
        assert_eq!(model.animations.len(), 1);
        assert_eq!(model.animations[0].animators.len(), 1);
        let names = uuid_name_map(&model);
        let anim = animation_from_bb("m", &model.animations[0], &names);
        assert_eq!(anim.parts.len(), 1);
        assert_eq!(anim.parts[0].part_name, "Head");
        assert!(!anim.parts[0].channels.is_empty());
    }
}
