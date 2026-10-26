use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use zip::ZipArchive;

use super::{BbModel, parse_gzip_compound};

const AVATAR_JSON: &str = "avatar.json";

#[derive(Clone, Debug)]
pub struct AvatarPackage {
    pub source: AvatarSource,
    pub json: AvatarJson,
    pub root: PathBuf,
    pub models: Vec<BbModel>,
    pub metadata_nbt: Option<super::NbtCompound>,
    pub avatar_png: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub enum AvatarSource {
    Directory(PathBuf),
    Zip(PathBuf),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AvatarJson {
    pub name: Option<String>,
    pub authors: Option<Vec<String>>,
    pub version: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub auto_scripts: Option<Vec<String>>,
    pub scripts: Option<Vec<String>>,
    pub models: Option<Vec<String>>,
    pub emojis: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub fn discover_package(path: &Path) -> io::Result<AvatarSource> {
    if path.is_dir() {
        if path.join(AVATAR_JSON).is_file() {
            return Ok(AvatarSource::Directory(path.to_path_buf()));
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing {AVATAR_JSON} in directory"),
        ));
    }

    if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        if archive.by_name(AVATAR_JSON).is_ok() {
            return Ok(AvatarSource::Zip(path.to_path_buf()));
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing {AVATAR_JSON} in zip"),
        ));
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "path must be a directory or .zip avatar package",
    ))
}

pub fn load_avatar_json(raw: &str) -> Result<AvatarJson, serde_json::Error> {
    serde_json::from_str(raw)
}

pub fn load_package(source: &AvatarSource) -> io::Result<AvatarPackage> {
    match source {
        AvatarSource::Directory(root) => load_from_directory(root),
        AvatarSource::Zip(path) => load_from_zip(path),
    }
}

fn load_from_directory(root: &Path) -> io::Result<AvatarPackage> {
    let json_path = root.join(AVATAR_JSON);
    let raw = fs::read_to_string(&json_path)?;
    let json = load_avatar_json(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

    let models = load_models_from_dir(root, json.models.as_deref())?;
    let metadata_nbt = load_optional_nbt(root.join("avatar.nbt"))?;
    let avatar_png = fs::read(root.join("avatar.png")).ok();

    Ok(AvatarPackage {
        source: AvatarSource::Directory(root.to_path_buf()),
        json,
        root: root.to_path_buf(),
        models,
        metadata_nbt,
        avatar_png,
    })
}

fn load_from_zip(path: &Path) -> io::Result<AvatarPackage> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

    let raw = read_zip_text(&mut archive, AVATAR_JSON)?;
    let json = load_avatar_json(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

    let model_names = {
        let listed = json.models.clone().unwrap_or_default();
        if !listed.is_empty() {
            listed
        } else {
            discover_bbmodel_names_in_zip(&mut archive)
        }
    };
    let mut models = Vec::new();
    for name in &model_names {
        let model_path = normalize_zip_path(name);
        if let Ok(text) = read_zip_text(&mut archive, &model_path) {
            if let Ok(model) = super::BbModel::from_json_str(&text) {
                models.push(model);
            }
        }
    }

    let metadata_nbt = read_zip_bytes(&mut archive, "avatar.nbt")
        .ok()
        .and_then(|bytes| parse_gzip_compound(&bytes).ok());
    let avatar_png = read_zip_bytes(&mut archive, "avatar.png").ok();

    Ok(AvatarPackage {
        source: AvatarSource::Zip(path.to_path_buf()),
        json,
        root: path.to_path_buf(),
        models,
        metadata_nbt,
        avatar_png,
    })
}

fn load_models_from_dir(root: &Path, names: Option<&[String]>) -> io::Result<Vec<BbModel>> {
    let discovered = discover_bbmodel_names(root)?;
    let names: Vec<String> = match names {
        Some(listed) if !listed.is_empty() => listed.to_vec(),
        _ => discovered,
    };

    let mut models = Vec::new();
    let mut errors = Vec::new();
    for name in &names {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        match super::BbModel::from_json_str(&raw) {
            Ok(mut model) => {
                let stem = Path::new(name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(name)
                    .to_string();
                model.file_stem = Some(stem);
                models.push(model);
            }
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    if models.is_empty() && !errors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bbmodel parse failed: {}", errors.join("; ")),
        ));
    }
    Ok(models)
}

fn discover_bbmodel_names(root: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bbmodel") {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn load_optional_nbt(path: PathBuf) -> io::Result<Option<super::NbtCompound>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    parse_gzip_compound(&bytes)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

fn read_zip_text(archive: &mut ZipArchive<fs::File>, name: &str) -> io::Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err.to_string()))?;
    let mut raw = String::new();
    file.read_to_string(&mut raw)?;
    Ok(raw)
}

fn read_zip_bytes(archive: &mut ZipArchive<fs::File>, name: &str) -> io::Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err.to_string()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn normalize_zip_path(name: &str) -> String {
    name.replace('\\', "/")
}

fn discover_bbmodel_names_in_zip(archive: &mut ZipArchive<fs::File>) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let Ok(file) = archive.by_index(i) else {
            continue;
        };
        let name = file.name().replace('\\', "/");
        if name.ends_with(".bbmodel") && !name.contains("..") {
            names.push(name);
        }
    }
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_avatar_json() {
        let raw =
            r##"{"name":"Test Avatar","authors":["Alice"],"version":"1.0.0","color":"#ff00ff"}"##;
        let json = load_avatar_json(raw).expect("avatar.json parse");
        assert_eq!(json.name.as_deref(), Some("Test Avatar"));
        assert_eq!(json.authors.as_ref().map(|a| a.len()), Some(1));
        assert_eq!(json.version.as_deref(), Some("1.0.0"));
        assert_eq!(json.color.as_deref(), Some("#ff00ff"));
    }

    #[test]
    fn load_vector_package_if_present() {
        let path = Path::new(r"C:\Users\muxam\Desktop\ch33t\vector");
        if !path.is_dir() {
            return;
        }
        let source = discover_package(path).expect("discover vector");
        let package = load_package(&source).expect("load vector");
        assert!(
            !package.models.is_empty(),
            "vector should auto-discover model.bbmodel"
        );
        let model = &package.models[0];
        assert!(
            !model.elements.is_empty(),
            "expected at least one element, got {}",
            model.elements.len()
        );
        let parts = crate::avatar::render::mesh::meshes_from_model(model);
        assert!(
            !parts.is_empty() && parts.iter().any(|p| !p.mesh.vertices.is_empty()),
            "expected tessellated geometry from vector model"
        );
        let tree = crate::avatar::model::build_model_tree(model);
        let flat = crate::avatar::model::flatten_textured_meshes(&tree);
        assert!(
            !flat.is_empty(),
            "expected hierarchical flatten to produce meshes"
        );

        fn find_group(part: &crate::avatar::model::ModelPart, name: &str) -> bool {
            if part.name == name {
                return !part.children.is_empty()
                    && part.parent_type != crate::avatar::model::ParentType::None;
            }
            part.children.iter().any(|c| find_group(c, name))
        }
        assert!(find_group(&tree, "LeftArm"), "LeftArm ParentType group");
        assert!(find_group(&tree, "Head"), "Head ParentType group");

        let mut posed = tree.clone();
        let pose = crate::avatar::model::VanillaPose::from_render_state(
            30.0, 45.0, 2.0, 1.0, 1.0, false, 0.0,
        );
        posed.reset_animation();
        crate::avatar::model::apply_vanilla_pose(&mut posed, &pose);
        let moved = crate::avatar::model::flatten_textured_meshes(&posed);
        let sum = |parts: &[crate::avatar::render::mesh::TexturedMesh]| {
            parts
                .iter()
                .flat_map(|p| p.mesh.vertices.iter())
                .map(|v| v.x + v.y * 2.0 + v.z * 3.0)
                .sum::<f32>()
        };
        assert!(
            (sum(&flat) - sum(&moved)).abs() > 0.01,
            "vector vanilla pose must move verts"
        );
        let _ = model.textures.len();
    }

}
