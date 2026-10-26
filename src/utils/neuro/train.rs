use super::{features::FeatureSample, linear_model, store};

pub use linear_model::{clear_training_progress, training_heartbeat_ms, training_progress};

pub struct TrainResult {
    pub samples: usize,
    pub final_loss: f32,
    pub val_loss: f32,
}

pub fn train_from_dataset(
    _epochs: usize,
    _batch_size: usize,
    _learning_rate: f64,
) -> Result<TrainResult, String> {
    let samples = store::read_all_samples()?;
    let result = linear_model::train_from_samples(&samples)?;
    Ok(TrainResult {
        samples: result.samples,
        final_loss: result.final_loss,
        val_loss: result.final_loss,
    })
}
