use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::{
    features::{FEATURE_DIM, FeatureSample, LABEL_DIM, is_useful_sample, sample_weight},
    network::{self, AimNetwork, NetworkGrads, zero_context, zero_hidden},
    store,
};

const MAX_FRAMES: usize = 40_000;
const EPOCHS: usize = 96;
const BASE_LR: f32 = 0.0016;
const WEIGHT_DECAY: f32 = 1e-4;
const MAX_SEQ: usize = 64;
const MIN_USEFUL: usize = 64;
const AUG_COPIES: usize = 1;

static TRAIN_EPOCH: AtomicU32 = AtomicU32::new(0);
static TRAIN_EPOCHS_TOTAL: AtomicU32 = AtomicU32::new(0);
static TRAIN_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);

pub fn training_progress() -> (u32, u32) {
    (
        TRAIN_EPOCH.load(Ordering::Acquire),
        TRAIN_EPOCHS_TOTAL.load(Ordering::Acquire),
    )
}

pub fn training_heartbeat_ms() -> u64 {
    TRAIN_HEARTBEAT_MS.load(Ordering::Acquire)
}

pub fn clear_training_progress() {
    TRAIN_EPOCH.store(0, Ordering::Release);
    TRAIN_EPOCHS_TOTAL.store(0, Ordering::Release);
    TRAIN_HEARTBEAT_MS.store(0, Ordering::Release);
}

pub struct TrainResult {
    pub samples: usize,
    pub final_loss: f32,
}

struct TrainFrame {
    features: [f32; FEATURE_DIM],
    labels: [f32; LABEL_DIM],
    weight: f32,
    session_end: bool,
}

struct AdamState {
    m_enc_w: Vec<f32>,
    v_enc_w: Vec<f32>,
    m_enc_b: Vec<f32>,
    v_enc_b: Vec<f32>,
    m_enc_ln_g: Vec<f32>,
    v_enc_ln_g: Vec<f32>,
    m_enc_ln_b: Vec<f32>,
    v_enc_ln_b: Vec<f32>,
    m_gru_w_ir: Vec<f32>,
    v_gru_w_ir: Vec<f32>,
    m_gru_w_iz: Vec<f32>,
    v_gru_w_iz: Vec<f32>,
    m_gru_w_in: Vec<f32>,
    v_gru_w_in: Vec<f32>,
    m_gru_w_hr: Vec<f32>,
    v_gru_w_hr: Vec<f32>,
    m_gru_w_hz: Vec<f32>,
    v_gru_w_hz: Vec<f32>,
    m_gru_w_hn: Vec<f32>,
    v_gru_w_hn: Vec<f32>,
    m_gru_b_r: Vec<f32>,
    v_gru_b_r: Vec<f32>,
    m_gru_b_z: Vec<f32>,
    v_gru_b_z: Vec<f32>,
    m_gru_b_n: Vec<f32>,
    v_gru_b_n: Vec<f32>,
    m_head1_w: Vec<f32>,
    v_head1_w: Vec<f32>,
    m_head1_b: Vec<f32>,
    v_head1_b: Vec<f32>,
    m_head2_w: Vec<f32>,
    v_head2_w: Vec<f32>,
    m_head2_b: Vec<f32>,
    v_head2_b: Vec<f32>,
}

impl AdamState {
    fn new(model: &AimNetwork) -> Self {
        let z = |n: usize| vec![0.0; n];
        Self {
            m_enc_w: z(model.enc_w.len()),
            v_enc_w: z(model.enc_w.len()),
            m_enc_b: z(model.enc_b.len()),
            v_enc_b: z(model.enc_b.len()),
            m_enc_ln_g: z(model.enc_ln_g.len()),
            v_enc_ln_g: z(model.enc_ln_g.len()),
            m_enc_ln_b: z(model.enc_ln_b.len()),
            v_enc_ln_b: z(model.enc_ln_b.len()),
            m_gru_w_ir: z(model.gru_w_ir.len()),
            v_gru_w_ir: z(model.gru_w_ir.len()),
            m_gru_w_iz: z(model.gru_w_iz.len()),
            v_gru_w_iz: z(model.gru_w_iz.len()),
            m_gru_w_in: z(model.gru_w_in.len()),
            v_gru_w_in: z(model.gru_w_in.len()),
            m_gru_w_hr: z(model.gru_w_hr.len()),
            v_gru_w_hr: z(model.gru_w_hr.len()),
            m_gru_w_hz: z(model.gru_w_hz.len()),
            v_gru_w_hz: z(model.gru_w_hz.len()),
            m_gru_w_hn: z(model.gru_w_hn.len()),
            v_gru_w_hn: z(model.gru_w_hn.len()),
            m_gru_b_r: z(model.gru_b_r.len()),
            v_gru_b_r: z(model.gru_b_r.len()),
            m_gru_b_z: z(model.gru_b_z.len()),
            v_gru_b_z: z(model.gru_b_z.len()),
            m_gru_b_n: z(model.gru_b_n.len()),
            v_gru_b_n: z(model.gru_b_n.len()),
            m_head1_w: z(model.head1_w.len()),
            v_head1_w: z(model.head1_w.len()),
            m_head1_b: z(model.head1_b.len()),
            v_head1_b: z(model.head1_b.len()),
            m_head2_w: z(model.head2_w.len()),
            v_head2_w: z(model.head2_w.len()),
            m_head2_b: z(model.head2_b.len()),
            v_head2_b: z(model.head2_b.len()),
        }
    }
}

pub fn train_from_samples(samples: &[FeatureSample]) -> Result<TrainResult, String> {
    clear_training_progress();
    touch_heartbeat();

    let sessions = split_sessions(samples);
    let mut frames = collect_weighted_frames(&sessions);
    if frames.len() < MIN_USEFUL {
        return Err(format!(
            "not enough useful samples ({}/{MIN_USEFUL})",
            frames.len()
        ));
    }

    
    if frames.len() > MAX_FRAMES {
        shuffle_frames(&mut frames, 42);
        frames.truncate(MAX_FRAMES);
    }

    
    let mut augmented = frames.clone();
    for copy in 0..AUG_COPIES {
        for frame in &frames {
            augmented.push(augment_frame(frame, 100 + copy as u64));
        }
    }
    frames = augmented;

    let val_count = (frames.len() / 7)
        .max(32)
        .min(frames.len().saturating_sub(MIN_USEFUL));
    let split = frames.len() - val_count;
    shuffle_frames(&mut frames, 7);
    let (train_frames, val_frames) = frames.split_at(split);

    let mut train_sessions = window_sessions(&group_sessions(train_frames));
    let val_sessions = window_sessions(&group_sessions(val_frames));

    let mut model = AimNetwork::new_random(0xA11CE);
    let mut adam = AdamState::new(&model);
    let mut grads = NetworkGrads::zeros(&model);
    let mut best_val = f32::MAX;
    let mut best_model = model.clone();
    let mut stale_epochs = 0usize;
    let patience = 14usize;

    TRAIN_EPOCHS_TOTAL.store(EPOCHS as u32, Ordering::Release);

    for epoch in 0..EPOCHS {
        TRAIN_EPOCH.store(epoch as u32 + 1, Ordering::Release);
        touch_heartbeat();

        let lr = cosine_lr(BASE_LR, epoch, EPOCHS);
        shuffle_sessions(&mut train_sessions, 42 + epoch as u64);

        let mut train_loss = 0.0_f32;
        let mut batches = 0usize;
        for chunk in train_sessions.chunks(6) {
            grads.clear();
            let mut batch_loss = 0.0_f32;
            for session in chunk {
                batch_loss += train_session(&model, session, &mut grads);
            }
            adam_step(&mut model, &mut adam, &grads, lr, chunk.len());
            train_loss += batch_loss / chunk.len().max(1) as f32;
            batches += 1;
            touch_heartbeat();
        }
        let _train_loss = train_loss / batches.max(1) as f32;

        let val_loss = eval_session_loss(&model, &val_sessions);
        touch_heartbeat();

        if val_loss < best_val {
            best_val = val_loss;
            best_model = model.clone();
            stale_epochs = 0;
        } else {
            stale_epochs += 1;
            if stale_epochs >= patience {
                break;
            }
        }
    }

    network::save_to_disk(&best_model)?;
    let mut manifest = store::load_manifest(samples.len() as u64);
    manifest.model_ready = true;
    manifest.feature_layout_hash = super::features::FEATURE_LAYOUT_HASH;
    manifest.last_error.clear();
    store::save_manifest(&manifest)?;

    clear_training_progress();
    Ok(TrainResult {
        samples: frames.len(),
        final_loss: best_val,
    })
}

fn train_session(model: &AimNetwork, session: &[TrainFrame], grads: &mut NetworkGrads) -> f32 {
    let mut hidden = zero_hidden();
    let mut context = zero_context();
    let mut prev_pred = [0.0_f32; LABEL_DIM];
    let mut loss = 0.0_f32;
    for frame in session {
        let (step, cache) = model.forward_train(&frame.features, &context, &hidden);
        loss += network::backward_step(
            model,
            &cache,
            &frame.labels,
            frame.weight,
            &prev_pred,
            grads,
        );
        hidden = step.hidden;
        context = step.context;
        prev_pred = step.output;
    }
    loss / session.len().max(1) as f32
}

fn eval_session_loss(model: &AimNetwork, sessions: &[Vec<TrainFrame>]) -> f32 {
    if sessions.is_empty() {
        return f32::MAX;
    }
    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for session in sessions {
        let mut hidden = zero_hidden();
        let mut context = zero_context();
        for frame in session {
            let step = model.forward_step(&frame.features, &context, &hidden);
            hidden = step.hidden;
            context = step.context;
            let mut err = 0.0_f32;
            for k in 0..LABEL_DIM {
                let d = step.output[k] - frame.labels[k];
                err += d * d;
            }
            sum += err * frame.weight;
            count += 1;
        }
    }
    sum / count.max(1) as f32
}

fn group_sessions(frames: &[TrainFrame]) -> Vec<Vec<TrainFrame>> {
    let mut sessions: Vec<Vec<TrainFrame>> = Vec::new();
    let mut current = Vec::new();
    for frame in frames {
        current.push(frame.clone());
        if frame.session_end {
            if current.len() >= 2 {
                sessions.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        sessions.push(current);
    }
    if sessions.is_empty() && !frames.is_empty() {
        sessions.push(frames.to_vec());
    }
    sessions
}



fn window_sessions(sessions: &[Vec<TrainFrame>]) -> Vec<Vec<TrainFrame>> {
    let mut out = Vec::new();
    let stride = MAX_SEQ / 2;
    for session in sessions {
        if session.len() <= MAX_SEQ {
            out.push(session.clone());
            continue;
        }
        let mut start = 0usize;
        while start < session.len() {
            let end = (start + MAX_SEQ).min(session.len());
            out.push(session[start..end].to_vec());
            if end == session.len() {
                break;
            }
            start += stride.max(1);
        }
    }
    out
}

fn shuffle_sessions(sessions: &mut [Vec<TrainFrame>], seed: u64) {
    let mut state = seed.max(1);
    for i in (1..sessions.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        sessions.swap(i, j);
    }
}

fn adam_step(
    model: &mut AimNetwork,
    adam: &mut AdamState,
    grads: &NetworkGrads,
    lr: f32,
    batch_len: usize,
) {
    let scale = 1.0 / batch_len as f32;
    let b1 = 0.9_f32;
    let b2 = 0.999_f32;
    let eps = 1e-8_f32;
    let t = TRAIN_EPOCH.load(Ordering::Relaxed).max(1) as f32;
    let bc1 = 1.0 - b1.powf(t);
    let bc2 = 1.0 - b2.powf(t);

    macro_rules! update {
        ($param:expr, $grad:expr, $m:expr, $v:expr) => {{
            for i in 0..$param.len() {
                let g = $grad[i] * scale + WEIGHT_DECAY * $param[i];
                $m[i] = b1 * $m[i] + (1.0 - b1) * g;
                $v[i] = b2 * $v[i] + (1.0 - b2) * g * g;
                let m_hat = $m[i] / bc1;
                let v_hat = $v[i] / bc2;
                $param[i] -= lr * m_hat / (v_hat.sqrt() + eps);
            }
        }};
    }

    update!(model.enc_w, grads.enc_w, adam.m_enc_w, adam.v_enc_w);
    update!(model.enc_b, grads.enc_b, adam.m_enc_b, adam.v_enc_b);
    update!(
        model.enc_ln_g,
        grads.enc_ln_g,
        adam.m_enc_ln_g,
        adam.v_enc_ln_g
    );
    update!(
        model.enc_ln_b,
        grads.enc_ln_b,
        adam.m_enc_ln_b,
        adam.v_enc_ln_b
    );
    update!(
        model.gru_w_ir,
        grads.gru_w_ir,
        adam.m_gru_w_ir,
        adam.v_gru_w_ir
    );
    update!(
        model.gru_w_iz,
        grads.gru_w_iz,
        adam.m_gru_w_iz,
        adam.v_gru_w_iz
    );
    update!(
        model.gru_w_in,
        grads.gru_w_in,
        adam.m_gru_w_in,
        adam.v_gru_w_in
    );
    update!(
        model.gru_w_hr,
        grads.gru_w_hr,
        adam.m_gru_w_hr,
        adam.v_gru_w_hr
    );
    update!(
        model.gru_w_hz,
        grads.gru_w_hz,
        adam.m_gru_w_hz,
        adam.v_gru_w_hz
    );
    update!(
        model.gru_w_hn,
        grads.gru_w_hn,
        adam.m_gru_w_hn,
        adam.v_gru_w_hn
    );
    update!(model.gru_b_r, grads.gru_b_r, adam.m_gru_b_r, adam.v_gru_b_r);
    update!(model.gru_b_z, grads.gru_b_z, adam.m_gru_b_z, adam.v_gru_b_z);
    update!(model.gru_b_n, grads.gru_b_n, adam.m_gru_b_n, adam.v_gru_b_n);
    update!(model.head1_w, grads.head1_w, adam.m_head1_w, adam.v_head1_w);
    update!(model.head1_b, grads.head1_b, adam.m_head1_b, adam.v_head1_b);
    update!(model.head2_w, grads.head2_w, adam.m_head2_w, adam.v_head2_w);
    update!(model.head2_b, grads.head2_b, adam.m_head2_b, adam.v_head2_b);
}

fn cosine_lr(base: f32, epoch: usize, total: usize) -> f32 {
    let t = epoch as f32 / total.max(1) as f32;
    let warm = if epoch < 4 {
        0.35 + 0.65 * (epoch as f32 / 4.0)
    } else {
        1.0
    };
    warm * base * (0.12 + 0.88 * (0.5 + 0.5 * (std::f32::consts::PI * t).cos()))
}

fn split_sessions(samples: &[FeatureSample]) -> Vec<Vec<FeatureSample>> {
    let mut sessions = Vec::new();
    let mut current = Vec::new();
    for sample in samples {
        if !is_useful_sample(sample) {
            continue;
        }
        current.push(*sample);
        if sample.session_end {
            if current.len() >= 2 {
                sessions.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        sessions.push(current);
    }
    if sessions.is_empty() {
        let useful: Vec<_> = samples.iter().copied().filter(is_useful_sample).collect();
        if useful.len() >= MIN_USEFUL {
            sessions.push(useful);
        }
    }
    sessions
}

fn collect_weighted_frames(sessions: &[Vec<FeatureSample>]) -> Vec<TrainFrame> {
    let mut frames = Vec::new();
    for session in sessions {
        for sample in session {
            let err_mag = (sample.features[0].abs() + sample.features[1].abs()) as f64 * 45.0;
            frames.push(TrainFrame {
                features: sample.features,
                labels: sample.labels,
                weight: sample_weight(err_mag),
                session_end: sample.session_end,
            });
        }
    }
    frames
}

fn augment_frame(frame: &TrainFrame, seed: u64) -> TrainFrame {
    let mut rng = seed.max(1);
    let mut features = frame.features;
    let mut labels = frame.labels;
    for f in &mut features {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let noise = ((rng as f32) / (u32::MAX as f32) * 2.0 - 1.0) * 0.035;
        *f = (*f + noise).clamp(-3.0, 3.0);
    }
    for l in &mut labels {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let noise = ((rng as f32) / (u32::MAX as f32) * 2.0 - 1.0) * 0.02;
        *l += noise;
    }
    TrainFrame {
        features,
        labels,
        weight: frame.weight * 0.85,
        session_end: frame.session_end,
    }
}

fn shuffle_frames(frames: &mut [TrainFrame], seed: u64) {
    let mut state = seed.max(1);
    for i in (1..frames.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        frames.swap(i, j);
    }
}

fn touch_heartbeat() {
    TRAIN_HEARTBEAT_MS.store(unix_now_ms(), Ordering::Release);
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Clone for TrainFrame {
    fn clone(&self) -> Self {
        Self {
            features: self.features,
            labels: self.labels,
            weight: self.weight,
            session_end: self.session_end,
        }
    }
}
