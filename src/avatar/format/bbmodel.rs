use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct BbModel {
    pub name: String,
    
    pub file_stem: Option<String>,
    pub format_version: Option<String>,
    pub resolution: Option<BbVec2>,
    pub elements: Vec<BbElement>,
    pub groups: Vec<BbGroup>,
    pub textures: Vec<TextureRef>,
    pub animations: Vec<AnimationDef>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BbVec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BbVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BbElementKind {
    #[default]
    Cube,
    Mesh,
}

#[derive(Clone, Debug, Default)]
pub struct BbElement {
    pub name: String,
    pub uuid: Option<String>,
    pub kind: BbElementKind,
    pub from: BbVec3,
    pub to: BbVec3,
    pub origin: BbVec3,
    pub rotation: Option<BbRotation>,
    pub faces: HashMap<String, BbFace>,
    
    pub vertices: HashMap<String, BbVec3>,
}

#[derive(Clone, Debug, Default)]
pub struct BbFace {
    pub uv: [f32; 4],
    pub texture: Option<u32>,
    
    pub rotation: i32,
    
    pub vertex_ids: Vec<String>,
    
    pub vertex_uvs: HashMap<String, [f32; 2]>,
}

#[derive(Clone, Debug, Default)]
pub struct BbRotation {
    pub angle: f32,
    pub axis: String,
    pub origin: BbVec3,
    
    pub euler: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Default)]
pub struct BbGroup {
    pub name: String,
    pub uuid: Option<String>,
    pub origin: BbVec3,
    
    pub rotation: Option<[f32; 3]>,
    pub children: Vec<BbGroupChild>,
}

#[derive(Clone, Debug)]
pub enum BbGroupChild {
    Group(BbGroup),
    Element(String),
}

#[derive(Clone, Debug, Default)]
pub struct TextureRef {
    pub name: String,
    pub path: Option<String>,
    pub id: Option<String>,
    
    pub uv_width: f32,
    pub uv_height: f32,
    
    pub png_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub struct AnimationDef {
    pub name: String,
    pub length: f32,
    pub loop_mode: Option<String>,
    
    pub override_vanilla: bool,
    
    pub animators: Vec<PartAnimatorDef>,
    
    pub keyframes: Vec<KeyframeStub>,
}

#[derive(Clone, Debug, Default)]
pub struct PartAnimatorDef {
    pub id: String,
    pub rotation_global: bool,
    pub keyframes: Vec<BbKeyframeDef>,
}

#[derive(Clone, Debug, Default)]
pub struct BbKeyframeDef {
    pub channel: String,
    pub time: f32,
    pub interpolation: String,
    pub data: Value,
}

#[derive(Clone, Debug, Default)]
pub struct KeyframeStub {
    pub channel: String,
    pub time: f32,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
struct BbModelJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    meta: Option<BbMetaJson>,
    #[serde(default)]
    resolution: Option<BbResolutionJson>,
    #[serde(default)]
    elements: Vec<BbElementJson>,
    #[serde(default)]
    outliner: Vec<Value>,
    
    #[serde(default)]
    groups: Vec<BbGroupTableJson>,
    #[serde(default)]
    textures: Vec<TextureRefJson>,
    #[serde(default)]
    animations: Vec<AnimationDefJson>,
}

#[derive(Debug, Deserialize)]
struct BbGroupTableJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    origin: Option<[f32; 3]>,
    #[serde(default)]
    rotation: Option<[f32; 3]>,
}

#[derive(Debug, Deserialize)]
struct BbMetaJson {
    #[serde(default, rename = "format_version")]
    format_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BbResolutionJson {
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
}

#[derive(Debug, Deserialize)]
struct BbElementJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default, rename = "type")]
    element_type: Option<String>,
    #[serde(default)]
    from: [f32; 3],
    #[serde(default)]
    to: [f32; 3],
    #[serde(default)]
    origin: Option<[f32; 3]>,
    #[serde(default)]
    rotation: Option<BbRotationJson>,
    #[serde(default)]
    faces: HashMap<String, BbFaceJson>,
    #[serde(default)]
    vertices: HashMap<String, [f32; 3]>,
}


#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BbRotationJson {
    Euler([f32; 3]),
    AxisAngle {
        #[serde(default)]
        angle: f32,
        #[serde(default)]
        axis: Option<String>,
        #[serde(default)]
        origin: Option<[f32; 3]>,
    },
}

#[derive(Debug, Deserialize)]
struct BbFaceJson {
    #[serde(default)]
    uv: Value,
    #[serde(default)]
    texture: Option<i64>,
    #[serde(default)]
    rotation: Option<i64>,
    #[serde(default)]
    vertices: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct TextureRefJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    width: Option<f32>,
    #[serde(default)]
    height: Option<f32>,
    #[serde(default)]
    uv_width: Option<f32>,
    #[serde(default)]
    uv_height: Option<f32>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnimationDefJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    length: Option<f32>,
    #[serde(default, rename = "loop")]
    loop_mode: Option<String>,
    #[serde(default, rename = "override")]
    override_vanilla: Option<bool>,
    #[serde(default)]
    animators: HashMap<String, AnimatorJson>,
    #[serde(default)]
    keyframes: Vec<KeyframeStubJson>,
}

#[derive(Debug, Deserialize)]
struct AnimatorJson {
    #[serde(default)]
    keyframes: Vec<BbKeyframeJson>,
    #[serde(default)]
    rotation_global: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BbKeyframeJson {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    interpolation: Option<String>,
    #[serde(default)]
    time: Option<f32>,
    #[serde(default)]
    data_points: Vec<Value>,
    
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Deserialize)]
struct KeyframeStubJson {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    time: Option<f32>,
    #[serde(default)]
    data: Value,
}

impl BbModel {
    pub fn from_json_str(raw: &str) -> Result<Self, serde_json::Error> {
        let parsed: BbModelJson = serde_json::from_str(raw)?;
        Ok(parsed.into())
    }
}

impl From<BbModelJson> for BbModel {
    fn from(parsed: BbModelJson) -> Self {
        let mut group_meta: HashMap<String, BbGroupTableJson> = HashMap::new();
        for g in parsed.groups {
            if let Some(uuid) = g.uuid.clone() {
                group_meta.insert(uuid, g);
            }
        }
        let groups = parse_outliner(&parsed.outliner, &group_meta);
        Self {
            name: parsed.name.unwrap_or_else(|| "model".to_string()),
            file_stem: None,
            format_version: parsed.meta.and_then(|meta| meta.format_version),
            resolution: parsed.resolution.map(|res| BbVec2 {
                x: res.width,
                y: res.height,
            }),
            elements: parsed.elements.into_iter().map(Into::into).collect(),
            groups,
            textures: parsed.textures.into_iter().map(Into::into).collect(),
            animations: parsed.animations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BbElementJson> for BbElement {
    fn from(value: BbElementJson) -> Self {
        let kind = match value.element_type.as_deref() {
            Some("mesh") => BbElementKind::Mesh,
            _ => BbElementKind::Cube,
        };
        Self {
            name: value.name.unwrap_or_else(|| {
                if kind == BbElementKind::Mesh {
                    "mesh".to_string()
                } else {
                    "cube".to_string()
                }
            }),
            uuid: value.uuid,
            kind,
            from: vec3(value.from),
            to: vec3(value.to),
            origin: value.origin.map(vec3).unwrap_or_default(),
            rotation: value.rotation.map(Into::into),
            faces: value
                .faces
                .into_iter()
                .map(|(key, face)| (key, face.into()))
                .collect(),
            vertices: value
                .vertices
                .into_iter()
                .map(|(k, v)| (k, vec3(v)))
                .collect(),
        }
    }
}

impl From<BbRotationJson> for BbRotation {
    fn from(value: BbRotationJson) -> Self {
        match value {
            BbRotationJson::Euler(euler) => Self {
                angle: 0.0,
                axis: "euler".into(),
                origin: BbVec3::default(),
                euler: Some(euler),
            },
            BbRotationJson::AxisAngle {
                angle,
                axis,
                origin,
            } => Self {
                angle,
                axis: axis.unwrap_or_else(|| "y".to_string()),
                origin: origin.map(vec3).unwrap_or_default(),
                euler: None,
            },
        }
    }
}

impl From<BbFaceJson> for BbFace {
    fn from(value: BbFaceJson) -> Self {
        let texture = value
            .texture
            .and_then(|t| if t < 0 { None } else { Some(t as u32) });
        let vertex_uvs = parse_vertex_uvs(&value.uv);
        let uv = if vertex_uvs.is_empty() {
            parse_face_uv(&value.uv)
        } else {
            let mut u_min = f32::INFINITY;
            let mut v_min = f32::INFINITY;
            let mut u_max = f32::NEG_INFINITY;
            let mut v_max = f32::NEG_INFINITY;
            for [u, v] in vertex_uvs.values() {
                u_min = u_min.min(*u);
                v_min = v_min.min(*v);
                u_max = u_max.max(*u);
                v_max = v_max.max(*v);
            }
            [u_min, v_min, u_max, v_max]
        };
        let rotation = value.rotation.map(|r| r as i32).unwrap_or(0);
        Self {
            uv,
            texture,
            rotation,
            vertex_ids: value.vertices.unwrap_or_default(),
            vertex_uvs,
        }
    }
}

fn parse_vertex_uvs(value: &Value) -> HashMap<String, [f32; 2]> {
    let mut out = HashMap::new();
    let Some(obj) = value.as_object() else {
        return out;
    };
    for (key, point) in obj {
        let Some(pair) = point.as_array() else {
            continue;
        };
        if pair.len() < 2 {
            continue;
        }
        out.insert(
            key.clone(),
            [
                pair[0].as_f64().unwrap_or(0.0) as f32,
                pair[1].as_f64().unwrap_or(0.0) as f32,
            ],
        );
    }
    out
}

fn parse_face_uv(value: &Value) -> [f32; 4] {
    if let Some(arr) = value.as_array() {
        if arr.len() >= 4 {
            return [
                arr[0].as_f64().unwrap_or(0.0) as f32,
                arr[1].as_f64().unwrap_or(0.0) as f32,
                arr[2].as_f64().unwrap_or(0.0) as f32,
                arr[3].as_f64().unwrap_or(0.0) as f32,
            ];
        }
    }
    
    if let Some(obj) = value.as_object() {
        let mut u_min = f32::INFINITY;
        let mut v_min = f32::INFINITY;
        let mut u_max = f32::NEG_INFINITY;
        let mut v_max = f32::NEG_INFINITY;
        let mut any = false;
        for point in obj.values() {
            let Some(pair) = point.as_array() else {
                continue;
            };
            if pair.len() < 2 {
                continue;
            }
            let u = pair[0].as_f64().unwrap_or(0.0) as f32;
            let v = pair[1].as_f64().unwrap_or(0.0) as f32;
            u_min = u_min.min(u);
            v_min = v_min.min(v);
            u_max = u_max.max(u);
            v_max = v_max.max(v);
            any = true;
        }
        if any {
            return [u_min, v_min, u_max, v_max];
        }
    }
    [0.0, 0.0, 16.0, 16.0]
}

impl From<TextureRefJson> for TextureRef {
    fn from(value: TextureRefJson) -> Self {
        let uv_width = value
            .uv_width
            .or(value.width)
            .filter(|v| *v > 0.0)
            .unwrap_or(16.0);
        let uv_height = value
            .uv_height
            .or(value.height)
            .filter(|v| *v > 0.0)
            .unwrap_or(16.0);
        let png_bytes = value.source.as_deref().and_then(decode_data_url_image);
        Self {
            name: value.name.unwrap_or_default(),
            path: value.path,
            id: value.id,
            uv_width,
            uv_height,
            png_bytes,
        }
    }
}

fn decode_data_url_image(source: &str) -> Option<Vec<u8>> {
    const PNG: &str = "data:image/png;base64,";
    const JPG: &str = "data:image/jpeg;base64,";
    const JPG2: &str = "data:image/jpg;base64,";
    let b64 = source
        .strip_prefix(PNG)
        .or_else(|| source.strip_prefix(JPG))
        .or_else(|| source.strip_prefix(JPG2))?;
    decode_base64(b64)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < cleaned.len() {
        let a = val(cleaned[i])?;
        let b = val(cleaned[i + 1])?;
        let c = val(cleaned[i + 2])?;
        let d = val(cleaned[i + 3])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
        out.push((c << 6) | d);
        i += 4;
    }
    let rem = cleaned.len() - i;
    if rem == 2 {
        let a = val(cleaned[i])?;
        let b = val(cleaned[i + 1])?;
        out.push((a << 2) | (b >> 4));
    } else if rem == 3 {
        let a = val(cleaned[i])?;
        let b = val(cleaned[i + 1])?;
        let c = val(cleaned[i + 2])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
    } else if rem != 0 {
        return None;
    }
    Some(out)
}

impl From<AnimationDefJson> for AnimationDef {
    fn from(value: AnimationDefJson) -> Self {
        let mut animators = Vec::new();
        for (id, animator) in value.animators {
            animators.push(PartAnimatorDef {
                id,
                rotation_global: animator.rotation_global.unwrap_or(false),
                keyframes: animator.keyframes.into_iter().map(Into::into).collect(),
            });
        }
        Self {
            name: value.name.unwrap_or_default(),
            length: value.length.unwrap_or(0.0),
            loop_mode: value.loop_mode,
            override_vanilla: value.override_vanilla.unwrap_or(false),
            animators,
            keyframes: value.keyframes.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BbKeyframeJson> for BbKeyframeDef {
    fn from(value: BbKeyframeJson) -> Self {
        let data = value.data_points.first().cloned().unwrap_or(value.data);
        Self {
            channel: value.channel.unwrap_or_default(),
            time: value.time.unwrap_or(0.0),
            interpolation: value.interpolation.unwrap_or_else(|| "linear".to_string()),
            data,
        }
    }
}

impl From<KeyframeStubJson> for KeyframeStub {
    fn from(value: KeyframeStubJson) -> Self {
        Self {
            channel: value.channel.unwrap_or_default(),
            time: value.time.unwrap_or(0.0),
            data: value.data,
        }
    }
}

fn vec3(values: [f32; 3]) -> BbVec3 {
    BbVec3 {
        x: values[0],
        y: values[1],
        z: values[2],
    }
}

fn parse_outliner(nodes: &[Value], group_meta: &HashMap<String, BbGroupTableJson>) -> Vec<BbGroup> {
    nodes
        .iter()
        .filter_map(|node| parse_outliner_node(node, group_meta))
        .collect()
}

fn parse_outliner_node(
    node: &Value,
    group_meta: &HashMap<String, BbGroupTableJson>,
) -> Option<BbGroup> {
    if let Some(name) = node.as_str() {
        
        return Some(BbGroup {
            name: name.to_string(),
            ..Default::default()
        });
    }

    let obj = node.as_object()?;
    let uuid = obj.get("uuid").and_then(Value::as_str).map(str::to_string);
    let meta = uuid.as_ref().and_then(|id| group_meta.get(id));

    
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| meta.and_then(|m| m.name.clone()))
        .unwrap_or_else(|| "group".to_string());

    let origin = obj
        .get("origin")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut values = [0.0_f32; 3];
            for (idx, item) in arr.iter().take(3).enumerate() {
                values[idx] = item.as_f64().unwrap_or(0.0) as f32;
            }
            vec3(values)
        })
        .or_else(|| meta.and_then(|m| m.origin.map(vec3)))
        .unwrap_or_default();

    let rotation = obj
        .get("rotation")
        .and_then(|v| match v {
            Value::Array(arr) if arr.len() >= 3 => Some([
                arr[0].as_f64().unwrap_or(0.0) as f32,
                arr[1].as_f64().unwrap_or(0.0) as f32,
                arr[2].as_f64().unwrap_or(0.0) as f32,
            ]),
            _ => None,
        })
        .or_else(|| meta.and_then(|m| m.rotation));

    let children = obj
        .get("children")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|child| {
                    if let Some(id) = child.as_str() {
                        Some(BbGroupChild::Element(id.to_string()))
                    } else {
                        parse_outliner_node(child, group_meta).map(BbGroupChild::Group)
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Some(BbGroup {
        name,
        uuid,
        origin,
        rotation,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_bbmodel() {
        let raw = r#"{
            "meta": {"format_version": "4.10"},
            "name": "Demo",
            "resolution": {"width": 16, "height": 16},
            "elements": [{
                "name": "body",
                "from": [0, 0, 0],
                "to": [8, 8, 8],
                "faces": {
                    "north": {"uv": [0, 0, 8, 8], "texture": 0}
                }
            }],
            "outliner": ["body"],
            "textures": [{"name": "skin", "path": "skin.png"}]
        }"#;

        let model = BbModel::from_json_str(raw).expect("bbmodel parse");
        assert_eq!(model.name, "Demo");
        assert_eq!(model.format_version.as_deref(), Some("4.10"));
        assert_eq!(model.elements.len(), 1);
        assert_eq!(model.elements[0].name, "body");
        assert_eq!(model.textures.len(), 1);
        assert_eq!(model.textures[0].name, "skin");
        assert_eq!(model.textures[0].uv_width, 16.0);
    }

    #[test]
    fn decode_embedded_png_data_url() {
        
        let src = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let bytes = decode_data_url_image(src).expect("decode");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn parse_euler_rotation_array() {
        let raw = r#"{
            "name": "Arm",
            "elements": [{
                "name": "LeftArm",
                "from": [-4, 18, 0],
                "to": [-2.5, 24, 1.5],
                "origin": [-3, 22, 2],
                "rotation": [-12.2, -2.6, -22.2],
                "faces": { "north": {"uv": [0, 0, 4, 6], "texture": 0} }
            }],
            "textures": [{"name": "skin", "uv_width": 64, "uv_height": 64}]
        }"#;
        let model = BbModel::from_json_str(raw).expect("parse euler");
        let euler = model.elements[0]
            .rotation
            .as_ref()
            .and_then(|r| r.euler)
            .expect("euler");
        assert!((euler[0] - -12.2).abs() < 0.01);
    }

    #[test]
    fn blockbench5_groups_table_fills_outliner_names() {
        let raw = r#"{
            "meta": { "format_version": "5.0", "model_format": "free" },
            "name": "j",
            "elements": [{
                "name": "HeadCube",
                "uuid": "elem-1",
                "from": [-4,20,-4], "to": [4,28,4],
                "faces": { "north": { "uv": [0,0,8,8], "texture": 0 } }
            }],
            "groups": [{
                "name": "Head",
                "uuid": "group-1",
                "origin": [0, 24, 0],
                "rotation": [0, 0, 0],
                "children": []
            }],
            "outliner": [{
                "uuid": "group-1",
                "children": ["elem-1"]
            }],
            "textures": [{ "name": "t" }]
        }"#;
        let model = BbModel::from_json_str(raw).expect("parse");
        assert_eq!(model.groups.len(), 1);
        assert_eq!(model.groups[0].name, "Head");
        assert!((model.groups[0].origin.y - 24.0).abs() < 1e-3);
        let tree = crate::avatar::model::build_model_tree(&model);
        let head = tree.children.iter().find(|c| c.name == "Head");
        assert!(
            head.is_some(),
            "Head group missing: {:?}",
            tree.children.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
        assert_eq!(
            head.unwrap().parent_type,
            crate::avatar::model::ParentType::Head
        );
    }
}
