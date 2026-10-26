use serde::{Deserialize, Serialize};

pub const FEATURE_DIM: usize = 32;
pub const LABEL_DIM: usize = 4;
pub const SEQ_LEN: usize = 16;
pub const HIDDEN_DIM: usize = 192;


pub const FEATURE_LAYOUT_HASH: u64 = 0xA2_01_00_02;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FeatureSample {
    pub features: [f32; FEATURE_DIM],
    pub labels: [f32; LABEL_DIM],
    #[serde(default)]
    pub session_end: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FeatureBuilderInput {
    pub yaw_error: f64,
    pub pitch_error: f64,
    pub d_yaw_error: f64,
    pub d_pitch_error: f64,
    pub distance: f64,
    pub local_right: f64,
    pub local_up: f64,
    pub local_forward: f64,
    pub target_vel_right: f64,
    pub target_vel_forward: f64,
    pub target_vel_up: f64,
    pub screen_x_norm: f64,
    pub screen_y_norm: f64,
    pub mouse_step: f64,
    pub dt: f64,
    pub frames_since_acquire: f64,
    pub time_since_attack: f64,
    pub attack_held: bool,
    pub on_ground: bool,
    pub sprint: bool,
    pub sneak: bool,
    pub fall_distance: f64,
    pub attack_strength: f64,
    pub target_health: f64,
    pub prev_yaw_delta: f64,
    pub prev_pitch_delta: f64,
    pub accel_yaw_delta: f64,
    pub accel_pitch_delta: f64,
}

pub fn build_features(input: FeatureBuilderInput) -> [f32; FEATURE_DIM] {
    let error_mag =
        (input.yaw_error * input.yaw_error + input.pitch_error * input.pitch_error).sqrt();
    let error_angle = input.pitch_error.atan2(input.yaw_error);

    [
        (input.yaw_error / 90.0).clamp(-1.0, 1.0) as f32,
        (input.pitch_error / 45.0).clamp(-1.0, 1.0) as f32,
        (error_mag / 90.0).clamp(0.0, 1.0) as f32,
        error_angle.sin() as f32,
        error_angle.cos() as f32,
        (input.d_yaw_error / 6.0).clamp(-2.0, 2.0) as f32,
        (input.d_pitch_error / 4.0).clamp(-2.0, 2.0) as f32,
        (input.distance / 6.0).clamp(0.0, 2.0) as f32,
        (input.local_right / 3.0).clamp(-2.0, 2.0) as f32,
        (input.local_up / 2.5).clamp(-2.0, 2.0) as f32,
        (input.local_forward / 6.0).clamp(-1.0, 2.0) as f32,
        (input.target_vel_right / 0.8).clamp(-2.0, 2.0) as f32,
        (input.target_vel_forward / 0.8).clamp(-2.0, 2.0) as f32,
        (input.target_vel_up / 0.6).clamp(-2.0, 2.0) as f32,
        input.screen_x_norm.clamp(-1.5, 1.5) as f32,
        input.screen_y_norm.clamp(-1.5, 1.5) as f32,
        (input.mouse_step * 20.0).clamp(0.0, 2.0) as f32,
        (input.dt / 0.05).clamp(0.0, 3.0) as f32,
        (input.frames_since_acquire / 60.0).clamp(0.0, 3.0) as f32,
        (input.time_since_attack / 40.0).clamp(0.0, 3.0) as f32,
        if input.attack_held { 1.0 } else { 0.0 },
        if input.on_ground { 1.0 } else { 0.0 },
        if input.sprint { 1.0 } else { 0.0 },
        if input.sneak { 1.0 } else { 0.0 },
        (input.fall_distance / 3.0).clamp(0.0, 2.0) as f32,
        input.attack_strength.clamp(0.0, 1.0) as f32,
        (input.target_health / 20.0).clamp(0.0, 1.5) as f32,
        (input.prev_yaw_delta / 3.0).clamp(-2.0, 2.0) as f32,
        (input.prev_pitch_delta / 3.0).clamp(-2.0, 2.0) as f32,
        (input.accel_yaw_delta / 2.0).clamp(-2.0, 2.0) as f32,
        (input.accel_pitch_delta / 2.0).clamp(-2.0, 2.0) as f32,
        1.0,
    ]
}


pub fn decompose_delta(
    human_d_yaw: f64,
    human_d_pitch: f64,
    yaw_error: f64,
    pitch_error: f64,
) -> Option<([f32; LABEL_DIM], f32)> {
    let err_mag = (yaw_error * yaw_error + pitch_error * pitch_error).sqrt();
    if err_mag < 1.25 {
        return None;
    }

    let human_mag = (human_d_yaw * human_d_yaw + human_d_pitch * human_d_pitch).sqrt();
    if human_mag < 0.04 {
        return None;
    }

    let ux = yaw_error / err_mag;
    let uy = pitch_error / err_mag;
    let along = human_d_yaw * ux + human_d_pitch * uy;
    if along <= 0.0 {
        return None;
    }

    let along_yaw = ux * along;
    let along_pitch = uy * along;
    let perp_yaw = human_d_yaw - along_yaw;
    let perp_pitch = human_d_pitch - along_pitch;

    let labels = encode_labels(along_yaw, along_pitch, perp_yaw, perp_pitch);
    let weight = sample_weight(err_mag);
    Some((labels, weight))
}

pub fn encode_labels(
    along_yaw: f64,
    along_pitch: f64,
    perp_yaw: f64,
    perp_pitch: f64,
) -> [f32; LABEL_DIM] {
    [
        (along_yaw / 3.0).clamp(-3.0, 3.0) as f32,
        (along_pitch / 3.0).clamp(-3.0, 3.0) as f32,
        (perp_yaw / 1.5).clamp(-2.0, 2.0) as f32,
        (perp_pitch / 1.5).clamp(-2.0, 2.0) as f32,
    ]
}

pub fn decode_labels(labels: [f32; LABEL_DIM]) -> (f64, f64) {
    let along_yaw = labels[0] as f64 * 3.0;
    let along_pitch = labels[1] as f64 * 3.0;
    let perp_yaw = labels[2] as f64 * 1.5;
    let perp_pitch = labels[3] as f64 * 1.5;
    (along_yaw + perp_yaw, along_pitch + perp_pitch)
}

pub fn sample_weight(error_mag: f64) -> f32 {
    
    if error_mag < 1.5 {
        0.35
    } else if error_mag < 20.0 {
        1.0
    } else if error_mag < 45.0 {
        0.65
    } else {
        0.4
    }
}


pub fn clamp_delta_toward_error(
    pred_yaw: f64,
    pred_pitch: f64,
    yaw_error: f64,
    pitch_error: f64,
    deadzone: f64,
) -> (f64, f64) {
    let err_mag = (yaw_error * yaw_error + pitch_error * pitch_error).sqrt();
    if err_mag < deadzone {
        return (0.0, 0.0);
    }

    let dot = pred_yaw * yaw_error + pred_pitch * pitch_error;
    if dot <= 0.0 {
        let gain = 0.18_f64.min(1.0);
        return (yaw_error * gain, pitch_error * gain);
    }

    let along = (dot / err_mag).clamp(0.0, err_mag);
    let ux = yaw_error / err_mag;
    let uy = pitch_error / err_mag;
    (ux * along, uy * along)
}


pub fn blend_with_classic(
    pred_yaw: f64,
    pred_pitch: f64,
    yaw_error: f64,
    pitch_error: f64,
    confidence: f64,
) -> (f64, f64) {
    let classic_gain = 0.18;
    let classic_yaw = yaw_error * classic_gain;
    let classic_pitch = pitch_error * classic_gain;
    let c = confidence.clamp(0.0, 1.0);
    (
        pred_yaw * c + classic_yaw * (1.0 - c),
        pred_pitch * c + classic_pitch * (1.0 - c),
    )
}

pub fn world_delta_to_view_local(
    rel_x: f64,
    rel_y: f64,
    rel_z: f64,
    yaw_degrees: f64,
) -> (f64, f64, f64) {
    let yaw = yaw_degrees.to_radians();
    let sin_y = yaw.sin();
    let cos_y = yaw.cos();
    let local_right = rel_x * cos_y + rel_z * sin_y;
    let local_forward = -rel_x * sin_y + rel_z * cos_y;
    (local_right, rel_y, local_forward)
}

pub fn world_velocity_to_view_local(
    vel_x: f64,
    vel_y: f64,
    vel_z: f64,
    yaw_degrees: f64,
) -> (f64, f64, f64) {
    world_delta_to_view_local(vel_x, vel_y, vel_z, yaw_degrees)
}

pub fn screen_norm_from_error(yaw_error: f64, pitch_error: f64, half_fov: f64) -> (f64, f64) {
    let denom = half_fov.max(5.0);
    (yaw_error / denom, pitch_error / denom)
}

pub fn is_useful_sample(sample: &FeatureSample) -> bool {
    let yaw_err = sample.features[0].abs();
    let pitch_err = sample.features[1].abs();
    let label_mag = (sample.labels[0] * sample.labels[0]
        + sample.labels[1] * sample.labels[1]
        + sample.labels[2] * sample.labels[2]
        + sample.labels[3] * sample.labels[3])
        .sqrt();
    yaw_err + pitch_err > 0.02 && label_mag > 0.01
}

pub fn label_magnitude(sample: &FeatureSample) -> f32 {
    (sample.labels[0] * sample.labels[0]
        + sample.labels[1] * sample.labels[1]
        + sample.labels[2] * sample.labels[2]
        + sample.labels[3] * sample.labels[3])
        .sqrt()
}
