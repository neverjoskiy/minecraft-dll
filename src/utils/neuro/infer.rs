use std::sync::{Arc, Mutex};

use super::{
    features::{FEATURE_DIM, LABEL_DIM, decode_labels},
    linear_model::{self, AimNetwork},
    network::{CONTEXT_DIM, GRU_HIDDEN, zero_context, zero_hidden},
};


const EMA_ALPHA: f64 = 0.28;

const NEAR_ERROR: f64 = 4.0;

pub struct InferenceState {
    gru_hidden: [f32; GRU_HIDDEN],
    context: [f32; CONTEXT_DIM],
    ema_yaw: f64,
    ema_pitch: f64,
    prev_pred_yaw: f64,
    prev_pred_pitch: f64,
}

impl Default for InferenceState {
    fn default() -> Self {
        Self {
            gru_hidden: zero_hidden(),
            context: zero_context(),
            ema_yaw: 0.0,
            ema_pitch: 0.0,
            prev_pred_yaw: 0.0,
            prev_pred_pitch: 0.0,
        }
    }
}

pub struct NeuroInference {
    model: Mutex<Option<AimNetwork>>,
    state: Mutex<InferenceState>,
}

impl NeuroInference {
    pub fn new() -> Self {
        Self {
            model: Mutex::new(None),
            state: Mutex::new(InferenceState::default()),
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.model.lock() {
            *guard = None;
        }
        self.reset_state();
    }

    pub fn reset_state(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = InferenceState::default();
        }
    }

    pub fn load_from_disk(&self) -> Result<(), String> {
        if !linear_model::model_exists() {
            return Err("model file missing".into());
        }

        let model = linear_model::load_model()?;
        let mut guard = self
            .model
            .lock()
            .map_err(|_| "inference mutex poisoned".to_string())?;
        *guard = Some(model);
        self.reset_state();
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.model
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn predict(&self, features: [f32; FEATURE_DIM]) -> Option<(f64, f64, f64)> {
        let guard = self.model.lock().ok()?;
        let model = guard.as_ref()?;
        let mut state = self.state.lock().ok()?;

        let step = model.forward_step(&features, &state.context, &state.gru_hidden);
        state.gru_hidden = step.hidden;
        state.context = step.context;

        let (raw_yaw, raw_pitch) = decode_labels(step.output);
        let err_mag = (features[0] as f64 * 90.0).hypot(features[1] as f64 * 45.0);
        let alpha = adaptive_ema(err_mag);

        state.ema_yaw = alpha * raw_yaw + (1.0 - alpha) * state.ema_yaw;
        state.ema_pitch = alpha * raw_pitch + (1.0 - alpha) * state.ema_pitch;

        let confidence = estimate_confidence(
            state.ema_yaw,
            state.ema_pitch,
            features,
            state.prev_pred_yaw,
            state.prev_pred_pitch,
        );

        state.prev_pred_yaw = state.ema_yaw;
        state.prev_pred_pitch = state.ema_pitch;

        Some((state.ema_yaw, state.ema_pitch, confidence))
    }
}

fn adaptive_ema(error_mag: f64) -> f64 {
    let near = (error_mag / NEAR_ERROR).clamp(0.0, 1.0);
    
    (EMA_ALPHA * (0.35 + 0.65 * near)).clamp(0.08, EMA_ALPHA)
}

fn estimate_confidence(
    pred_yaw: f64,
    pred_pitch: f64,
    features: [f32; FEATURE_DIM],
    prev_yaw: f64,
    prev_pitch: f64,
) -> f64 {
    let pred_mag = (pred_yaw * pred_yaw + pred_pitch * pred_pitch).sqrt();
    let err_mag = (features[0] as f64 * 90.0).hypot(features[1] as f64 * 45.0);
    let jerk = (pred_yaw - prev_yaw).hypot(pred_pitch - prev_pitch);

    let mut confidence = 1.0_f64;
    if pred_mag > err_mag * 1.35 + 0.5 {
        confidence *= 0.45;
    }
    if jerk > 1.8 {
        confidence *= 0.55;
    }
    if err_mag < 1.5 {
        confidence *= 0.55;
    } else if err_mag < 3.0 {
        confidence *= 0.75;
    }
    confidence.clamp(0.12, 1.0)
}

pub type SharedInference = Arc<NeuroInference>;

pub fn shared_inference() -> SharedInference {
    Arc::new(NeuroInference::new())
}
