use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::config;

use super::features::{FEATURE_DIM, FEATURE_LAYOUT_HASH, FeatureSample, LABEL_DIM};

pub const PROFILE_NAME: &str = "default";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const DATASET_FILE: &str = "dataset.jsonl";
pub const MODEL_FILE_STEM: &str = "model";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NeuroManifest {
    pub profile_id: String,
    pub schema_version: u32,
    pub samples_count: u64,
    pub max_samples: u64,
    pub feature_layout_hash: u64,
    pub model_ready: bool,
    pub last_error: String,
}

impl NeuroManifest {
    pub fn new(max_samples: u64) -> Self {
        Self {
            profile_id: PROFILE_NAME.to_string(),
            schema_version: 2,
            samples_count: 0,
            max_samples,
            feature_layout_hash: FEATURE_LAYOUT_HASH,
            model_ready: false,
            last_error: String::new(),
        }
    }
}

#[derive(Deserialize)]
struct LooseFeatureSample {
    features: Vec<f32>,
    labels: Vec<f32>,
    #[serde(default)]
    session_end: bool,
}

pub fn profile_dir() -> Result<PathBuf, String> {
    let root = config::ensure_root_dir().map_err(|e| e.to_string())?;
    let path = root.join("neuro").join(PROFILE_NAME);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn manifest_path() -> Result<PathBuf, String> {
    Ok(profile_dir()?.join(MANIFEST_FILE))
}

pub fn dataset_path() -> Result<PathBuf, String> {
    Ok(profile_dir()?.join(DATASET_FILE))
}

pub fn model_path_stem() -> Result<PathBuf, String> {
    Ok(profile_dir()?.join(MODEL_FILE_STEM))
}

pub fn model_exists() -> bool {
    super::linear_model::model_exists() || legacy_burn_model_path().is_some()
}

fn legacy_burn_model_path() -> Option<PathBuf> {
    let stem = model_path_stem().ok()?;
    let mpk = stem.with_extension("mpk");
    if mpk.is_file() {
        return Some(mpk);
    }
    let mpk_gz = stem.with_extension("mpk.gz");
    if mpk_gz.is_file() {
        return Some(mpk_gz);
    }
    let parent = stem.parent()?;
    for entry in fs::read_dir(parent).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(MODEL_FILE_STEM)
            && (name.ends_with(".mpk") || name.ends_with(".mpk.gz"))
        {
            return Some(path);
        }
    }
    None
}

pub fn remove_model_files() -> Result<(), String> {
    let _ = super::linear_model::remove_model();
    if let Some(path) = legacy_burn_model_path() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn load_manifest(max_samples: u64) -> NeuroManifest {
    let Ok(path) = manifest_path() else {
        return NeuroManifest::new(max_samples);
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| NeuroManifest::new(max_samples)),
        Err(_) => NeuroManifest::new(max_samples),
    }
}

pub fn save_manifest(manifest: &NeuroManifest) -> Result<(), String> {
    let path = manifest_path()?;
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

pub fn append_samples(samples: &[FeatureSample]) -> Result<u64, String> {
    if samples.is_empty() {
        return Ok(load_manifest(20_000).samples_count);
    }

    let path = dataset_path()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;

    let mut batch: Vec<FeatureSample> = samples.to_vec();
    if let Some(last) = batch.last_mut() {
        last.session_end = true;
    }

    for sample in &batch {
        let line = serde_json::to_string(sample).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    file.flush().map_err(|e| e.to_string())?;

    let mut manifest = load_manifest(20_000);
    manifest.samples_count = manifest.samples_count.saturating_add(samples.len() as u64);
    save_manifest(&manifest)?;
    Ok(manifest.samples_count)
}

pub fn read_all_samples() -> Result<Vec<FeatureSample>, String> {
    let path = dataset_path()?;
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut samples = Vec::new();
    let mut skipped = 0usize;

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        match parse_sample_line(&line) {
            Some(sample) => samples.push(sample),
            None => skipped += 1,
        }
    }

    if samples.is_empty() && skipped > 0 {
        return Err(format!(
            "dataset has {skipped} incompatible rows (expected {FEATURE_DIM} features / {LABEL_DIM} labels); reset neuro profile and re-record"
        ));
    }

    Ok(samples)
}

fn parse_sample_line(line: &str) -> Option<FeatureSample> {
    let loose: LooseFeatureSample = serde_json::from_str(line).ok()?;
    if loose.features.len() != FEATURE_DIM || loose.labels.len() != LABEL_DIM {
        return None;
    }

    let mut features = [0.0_f32; FEATURE_DIM];
    let mut labels = [0.0_f32; LABEL_DIM];
    features.copy_from_slice(&loose.features);
    labels.copy_from_slice(&loose.labels);

    Some(FeatureSample {
        features,
        labels,
        session_end: loose.session_end,
    })
}

pub fn reset_profile(max_samples: u64) -> Result<(), String> {
    let dir = profile_dir()?;
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
    }
    save_manifest(&NeuroManifest::new(max_samples))
}
