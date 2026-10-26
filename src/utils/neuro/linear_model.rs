use std::fs;

pub use super::network::AimNetwork;
use super::{
    network::{self, MODEL_FILE, MODEL_SCHEMA_VERSION},
    store,
    trainer::{self, TrainResult},
};

pub fn model_path() -> Result<std::path::PathBuf, String> {
    Ok(store::profile_dir()?.join(MODEL_FILE))
}

pub fn model_exists() -> bool {
    model_path()
        .ok()
        .map(|path| path.is_file())
        .unwrap_or(false)
}

pub fn load_model() -> Result<AimNetwork, String> {
    let path = model_path()?;
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let model: AimNetwork = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if model.schema_version != MODEL_SCHEMA_VERSION {
        return Err("model schema outdated; press Reset neuro to retrain".into());
    }
    if model.feature_layout_hash != super::features::FEATURE_LAYOUT_HASH {
        return Err("model feature layout mismatch; reset neuro profile".into());
    }
    Ok(model)
}

pub fn remove_model() -> Result<(), String> {
    if let Ok(path) = model_path() {
        if path.is_file() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn train_from_samples(
    samples: &[super::features::FeatureSample],
) -> Result<TrainResult, String> {
    trainer::train_from_samples(samples)
}

pub type AimModelWeights = AimNetwork;

pub use trainer::{clear_training_progress, training_heartbeat_ms, training_progress};
