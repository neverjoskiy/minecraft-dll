use serde::{Deserialize, Serialize};

use super::features::{FEATURE_DIM, FEATURE_LAYOUT_HASH, LABEL_DIM};

pub const MODEL_SCHEMA_VERSION: u32 = 3;
pub const MODEL_FILE: &str = "aim_model.json";

pub const CONTEXT_DIM: usize = LABEL_DIM;
pub const INPUT_DIM: usize = FEATURE_DIM + CONTEXT_DIM;


pub const H1: usize = 224;
pub const GRU_HIDDEN: usize = 192;
pub const HEAD_HIDDEN: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AimNetwork {
    pub schema_version: u32,
    pub feature_layout_hash: u64,
    pub enc_w: Vec<f32>,
    pub enc_b: Vec<f32>,
    pub enc_ln_g: Vec<f32>,
    pub enc_ln_b: Vec<f32>,
    pub gru_w_ir: Vec<f32>,
    pub gru_w_iz: Vec<f32>,
    pub gru_w_in: Vec<f32>,
    pub gru_w_hr: Vec<f32>,
    pub gru_w_hz: Vec<f32>,
    pub gru_w_hn: Vec<f32>,
    pub gru_b_r: Vec<f32>,
    pub gru_b_z: Vec<f32>,
    pub gru_b_n: Vec<f32>,
    pub head1_w: Vec<f32>,
    pub head1_b: Vec<f32>,
    pub head2_w: Vec<f32>,
    pub head2_b: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct StepOutput {
    pub output: [f32; LABEL_DIM],
    pub hidden: [f32; GRU_HIDDEN],
    pub context: [f32; CONTEXT_DIM],
}

#[derive(Clone, Debug)]
pub struct StepCache {
    pub input: [f32; INPUT_DIM],
    pub enc_lin: Vec<f32>,
    pub enc_act: Vec<f32>,
    pub enc_ln: Vec<f32>,
    pub gru: GruCache,
    pub head1_lin: Vec<f32>,
    pub head1_act: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct GruCache {
    pub x: Vec<f32>,
    pub hidden_prev: [f32; GRU_HIDDEN],
    pub r: [f32; GRU_HIDDEN],
    pub z: [f32; GRU_HIDDEN],
    pub n: [f32; GRU_HIDDEN],
    pub hidden: [f32; GRU_HIDDEN],
}

impl AimNetwork {
    pub fn param_count() -> usize {
        INPUT_DIM * H1
            + H1 * 3
            + 3 * (H1 * GRU_HIDDEN + GRU_HIDDEN * GRU_HIDDEN + GRU_HIDDEN)
            + GRU_HIDDEN * HEAD_HIDDEN
            + HEAD_HIDDEN
            + HEAD_HIDDEN * LABEL_DIM
            + LABEL_DIM
    }

    pub fn new_random(seed: u64) -> Self {
        let mut rng = Rng(seed);
        Self {
            schema_version: MODEL_SCHEMA_VERSION,
            feature_layout_hash: FEATURE_LAYOUT_HASH,
            enc_w: rng.xavier(INPUT_DIM * H1, INPUT_DIM, H1),
            enc_b: vec![0.0; H1],
            enc_ln_g: vec![1.0; H1],
            enc_ln_b: vec![0.0; H1],
            gru_w_ir: rng.xavier(H1 * GRU_HIDDEN, H1, GRU_HIDDEN),
            gru_w_iz: rng.xavier(H1 * GRU_HIDDEN, H1, GRU_HIDDEN),
            gru_w_in: rng.xavier(H1 * GRU_HIDDEN, H1, GRU_HIDDEN),
            gru_w_hr: rng.xavier(GRU_HIDDEN * GRU_HIDDEN, GRU_HIDDEN, GRU_HIDDEN),
            gru_w_hz: rng.xavier(GRU_HIDDEN * GRU_HIDDEN, GRU_HIDDEN, GRU_HIDDEN),
            gru_w_hn: rng.xavier(GRU_HIDDEN * GRU_HIDDEN, GRU_HIDDEN, GRU_HIDDEN),
            gru_b_r: vec![0.0; GRU_HIDDEN],
            gru_b_z: vec![0.0; GRU_HIDDEN],
            gru_b_n: vec![0.0; GRU_HIDDEN],
            head1_w: rng.xavier(GRU_HIDDEN * HEAD_HIDDEN, GRU_HIDDEN, HEAD_HIDDEN),
            head1_b: vec![0.0; HEAD_HIDDEN],
            head2_w: rng.xavier(HEAD_HIDDEN * LABEL_DIM, HEAD_HIDDEN, LABEL_DIM),
            head2_b: vec![0.0; LABEL_DIM],
        }
    }

    pub fn forward_step(
        &self,
        features: &[f32; FEATURE_DIM],
        context: &[f32; CONTEXT_DIM],
        hidden: &[f32; GRU_HIDDEN],
    ) -> StepOutput {
        let (out, _) = self.forward_train(features, context, hidden);
        out
    }

    pub fn forward_train(
        &self,
        features: &[f32; FEATURE_DIM],
        context: &[f32; CONTEXT_DIM],
        hidden: &[f32; GRU_HIDDEN],
    ) -> (StepOutput, StepCache) {
        let mut input = [0.0_f32; INPUT_DIM];
        input[..FEATURE_DIM].copy_from_slice(features);
        input[FEATURE_DIM..].copy_from_slice(context);

        let enc_lin = matvec(&input, &self.enc_w, &self.enc_b, INPUT_DIM, H1);
        let enc_act = enc_lin.iter().map(|&v| gelu(v)).collect::<Vec<_>>();
        let enc_ln = layer_norm(&enc_act, &self.enc_ln_g, &self.enc_ln_b);
        let gru = gru_forward(
            &enc_ln,
            hidden,
            &self.gru_w_ir,
            &self.gru_w_iz,
            &self.gru_w_in,
            &self.gru_w_hr,
            &self.gru_w_hz,
            &self.gru_w_hn,
            &self.gru_b_r,
            &self.gru_b_z,
            &self.gru_b_n,
        );
        let head1_lin = matvec(
            &gru.hidden,
            &self.head1_w,
            &self.head1_b,
            GRU_HIDDEN,
            HEAD_HIDDEN,
        );
        let head1_act: Vec<f32> = head1_lin.iter().map(|&v| gelu(v)).collect();
        let output = matvec_arr(
            &head1_act,
            &self.head2_w,
            &self.head2_b,
            HEAD_HIDDEN,
            LABEL_DIM,
        );

        let step = StepOutput {
            output,
            hidden: gru.hidden,
            context: compress_output(&output),
        };
        let cache = StepCache {
            input,
            enc_lin,
            enc_act,
            enc_ln: enc_ln.clone(),
            gru,
            head1_lin,
            head1_act,
        };
        (step, cache)
    }
}

pub struct NetworkGrads {
    pub enc_w: Vec<f32>,
    pub enc_b: Vec<f32>,
    pub enc_ln_g: Vec<f32>,
    pub enc_ln_b: Vec<f32>,
    pub gru_w_ir: Vec<f32>,
    pub gru_w_iz: Vec<f32>,
    pub gru_w_in: Vec<f32>,
    pub gru_w_hr: Vec<f32>,
    pub gru_w_hz: Vec<f32>,
    pub gru_w_hn: Vec<f32>,
    pub gru_b_r: Vec<f32>,
    pub gru_b_z: Vec<f32>,
    pub gru_b_n: Vec<f32>,
    pub head1_w: Vec<f32>,
    pub head1_b: Vec<f32>,
    pub head2_w: Vec<f32>,
    pub head2_b: Vec<f32>,
}

impl NetworkGrads {
    pub fn zeros(model: &AimNetwork) -> Self {
        Self {
            enc_w: vec![0.0; model.enc_w.len()],
            enc_b: vec![0.0; model.enc_b.len()],
            enc_ln_g: vec![0.0; model.enc_ln_g.len()],
            enc_ln_b: vec![0.0; model.enc_ln_b.len()],
            gru_w_ir: vec![0.0; model.gru_w_ir.len()],
            gru_w_iz: vec![0.0; model.gru_w_iz.len()],
            gru_w_in: vec![0.0; model.gru_w_in.len()],
            gru_w_hr: vec![0.0; model.gru_w_hr.len()],
            gru_w_hz: vec![0.0; model.gru_w_hz.len()],
            gru_w_hn: vec![0.0; model.gru_w_hn.len()],
            gru_b_r: vec![0.0; model.gru_b_r.len()],
            gru_b_z: vec![0.0; model.gru_b_z.len()],
            gru_b_n: vec![0.0; model.gru_b_n.len()],
            head1_w: vec![0.0; model.head1_w.len()],
            head1_b: vec![0.0; model.head1_b.len()],
            head2_w: vec![0.0; model.head2_w.len()],
            head2_b: vec![0.0; model.head2_b.len()],
        }
    }

    pub fn clear(&mut self) {
        for slot in [
            &mut self.enc_w,
            &mut self.enc_b,
            &mut self.enc_ln_g,
            &mut self.enc_ln_b,
            &mut self.gru_w_ir,
            &mut self.gru_w_iz,
            &mut self.gru_w_in,
            &mut self.gru_w_hr,
            &mut self.gru_w_hz,
            &mut self.gru_w_hn,
            &mut self.gru_b_r,
            &mut self.gru_b_z,
            &mut self.gru_b_n,
            &mut self.head1_w,
            &mut self.head1_b,
            &mut self.head2_w,
            &mut self.head2_b,
        ] {
            slot.fill(0.0);
        }
    }
}

pub fn backward_step(
    model: &AimNetwork,
    cache: &StepCache,
    target: &[f32; LABEL_DIM],
    weight: f32,
    prev_pred: &[f32; LABEL_DIM],
    grads: &mut NetworkGrads,
) -> f32 {
    let output = matvec_arr(
        &cache.head1_act,
        &model.head2_w,
        &model.head2_b,
        HEAD_HIDDEN,
        LABEL_DIM,
    );

    let mut loss = 0.0_f32;
    let mut d_out = [0.0_f32; LABEL_DIM];
    for k in 0..LABEL_DIM {
        let diff = output[k] - target[k];
        let h = huber(diff, 0.35);
        loss += h * h * weight;
        d_out[k] = huber_grad(diff, 0.35) * 2.0 * weight / LABEL_DIM as f32;
        let smooth = output[k] - prev_pred[k];
        loss += smooth * smooth * 0.04 * weight;
        d_out[k] += 0.08 * smooth * weight / LABEL_DIM as f32;
    }
    loss += direction_loss(&output, target) * 0.2 * weight;
    add_direction_grad(&output, target, 0.2 * weight, &mut d_out);

    let mut d_head1 = vec![0.0_f32; HEAD_HIDDEN];
    matvec_backward(
        &cache.head1_act,
        &d_out,
        &model.head2_w,
        HEAD_HIDDEN,
        LABEL_DIM,
        &mut d_head1,
        &mut grads.head2_w,
        &mut grads.head2_b,
    );
    for j in 0..HEAD_HIDDEN {
        d_head1[j] *= gelu_grad(cache.head1_lin[j]);
    }
    let mut d_gru = [0.0_f32; GRU_HIDDEN];
    matvec_backward_arr(
        &cache.gru.hidden,
        &d_head1,
        &model.head1_w,
        GRU_HIDDEN,
        HEAD_HIDDEN,
        &mut d_gru,
        &mut grads.head1_w,
        &mut grads.head1_b,
    );

    let mut d_enc = vec![0.0_f32; H1];
    gru_backward(
        &cache.gru,
        &d_gru,
        &model.gru_w_ir,
        &model.gru_w_iz,
        &model.gru_w_in,
        &model.gru_w_hr,
        &model.gru_w_hz,
        &model.gru_w_hn,
        &mut d_enc,
        grads,
    );

    let mut d_enc_act = layer_norm_backward(
        &cache.enc_act,
        &d_enc,
        &model.enc_ln_g,
        &model.enc_ln_b,
        &mut grads.enc_ln_g,
        &mut grads.enc_ln_b,
    );
    for j in 0..H1 {
        d_enc_act[j] *= gelu_grad(cache.enc_lin[j]);
    }
    let mut d_input = [0.0_f32; INPUT_DIM];
    matvec_backward_arr(
        &cache.input,
        &d_enc_act,
        &model.enc_w,
        INPUT_DIM,
        H1,
        &mut d_input,
        &mut grads.enc_w,
        &mut grads.enc_b,
    );
    let _ = d_input;

    loss
}

pub fn zero_hidden() -> [f32; GRU_HIDDEN] {
    [0.0; GRU_HIDDEN]
}

pub fn zero_context() -> [f32; CONTEXT_DIM] {
    [0.0; CONTEXT_DIM]
}

pub fn compress_output(output: &[f32; LABEL_DIM]) -> [f32; CONTEXT_DIM] {
    let mut ctx = [0.0_f32; CONTEXT_DIM];
    for (d, &s) in ctx.iter_mut().zip(output.iter()) {
        *d = s.tanh();
    }
    ctx
}

fn huber(diff: f32, d: f32) -> f32 {
    if diff.abs() <= d {
        diff
    } else {
        d * diff.signum()
    }
}

fn huber_grad(diff: f32, d: f32) -> f32 {
    if diff.abs() <= d {
        diff
    } else {
        d * diff.signum()
    }
}

fn direction_loss(pred: &[f32; LABEL_DIM], target: &[f32; LABEL_DIM]) -> f32 {
    let pm = norm(pred);
    let tm = norm(target);
    if pm < 1e-5 || tm < 1e-5 {
        return 0.0;
    }
    let dot: f32 = pred.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
    1.0 - (dot / (pm * tm)).clamp(-1.0, 1.0)
}

fn add_direction_grad(
    pred: &[f32; LABEL_DIM],
    target: &[f32; LABEL_DIM],
    scale: f32,
    d_out: &mut [f32; LABEL_DIM],
) {
    let pm2 = norm_sq(pred).max(1e-6);
    let tm = norm(target).max(1e-6);
    let dot: f32 = pred.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
    let inv_pm = pm2.sqrt().recip();
    for k in 0..LABEL_DIM {
        d_out[k] += scale * (-target[k] / (tm * inv_pm) + dot * pred[k] / (pm2 * inv_pm));
    }
}

fn norm(v: &[f32; LABEL_DIM]) -> f32 {
    norm_sq(v).sqrt()
}

fn norm_sq(v: &[f32; LABEL_DIM]) -> f32 {
    v.iter().map(|x| x * x).sum()
}

fn matvec(input: &[f32], w: &[f32], b: &[f32], in_dim: usize, out_dim: usize) -> Vec<f32> {
    let mut out = vec![0.0; out_dim];
    for j in 0..out_dim {
        let mut s = b[j];
        for i in 0..in_dim {
            s += input[i] * w[j * in_dim + i];
        }
        out[j] = s;
    }
    out
}

fn matvec_arr(
    input: &[f32],
    w: &[f32],
    b: &[f32],
    in_dim: usize,
    out_dim: usize,
) -> [f32; LABEL_DIM] {
    let v = matvec(input, w, b, in_dim, out_dim);
    let mut out = [0.0_f32; LABEL_DIM];
    out[..out_dim].copy_from_slice(&v[..out_dim]);
    out
}

fn matvec_backward(
    input: &[f32],
    d_out: &[f32],
    w: &[f32],
    in_dim: usize,
    out_dim: usize,
    d_in: &mut [f32],
    d_w: &mut [f32],
    d_b: &mut [f32],
) {
    for j in 0..out_dim {
        d_b[j] += d_out[j];
        for i in 0..in_dim {
            d_w[j * in_dim + i] += d_out[j] * input[i];
            d_in[i] += d_out[j] * w[j * in_dim + i];
        }
    }
}

fn matvec_backward_arr<const IN: usize>(
    input: &[f32; IN],
    d_out: &[f32],
    w: &[f32],
    in_dim: usize,
    out_dim: usize,
    d_in: &mut [f32],
    d_w: &mut [f32],
    d_b: &mut [f32],
) {
    for j in 0..out_dim {
        d_b[j] += d_out[j];
        for i in 0..in_dim {
            d_w[j * in_dim + i] += d_out[j] * input[i];
            d_in[i] += d_out[j] * w[j * in_dim + i];
        }
    }
}

fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + (0.797_884_56 * (x + 0.044_715 * x * x * x)).tanh())
}

fn gelu_grad(x: f32) -> f32 {
    let t = (0.797_884_56 * (x + 0.044_715 * x * x * x)).tanh();
    let sech2 = 1.0 - t * t;
    0.5 * (1.0 + t) + 0.5 * x * sech2 * 0.797_884_56 * (1.0 + 0.134_145 * x * x)
}

fn layer_norm(x: &[f32], g: &[f32], b: &[f32]) -> Vec<f32> {
    let n = x.len() as f32;
    let mean = x.iter().sum::<f32>() / n;
    let var = x.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n + 1e-5;
    let inv = var.sqrt().recip();
    x.iter()
        .enumerate()
        .map(|(i, &v)| (v - mean) * inv * g[i] + b[i])
        .collect()
}

fn layer_norm_backward(
    x: &[f32],
    d_y: &[f32],
    g: &[f32],
    _b: &[f32],
    d_g: &mut [f32],
    d_b: &mut [f32],
) -> Vec<f32> {
    let n = x.len() as f32;
    let mean = x.iter().sum::<f32>() / n;
    let var = x.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n + 1e-5;
    let inv = var.sqrt().recip();
    let mut d_x = vec![0.0; x.len()];
    for i in 0..x.len() {
        let norm = (x[i] - mean) * inv;
        d_g[i] += d_y[i] * norm;
        d_b[i] += d_y[i];
        d_x[i] = d_y[i] * g[i] * inv;
    }
    d_x
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn gru_forward(
    x: &[f32],
    hidden: &[f32; GRU_HIDDEN],
    w_ir: &[f32],
    w_iz: &[f32],
    w_in: &[f32],
    w_hr: &[f32],
    w_hz: &[f32],
    w_hn: &[f32],
    b_r: &[f32],
    b_z: &[f32],
    b_n: &[f32],
) -> GruCache {
    let in_dim = x.len();
    let mut r = [0.0_f32; GRU_HIDDEN];
    let mut z = [0.0_f32; GRU_HIDDEN];
    let mut n = [0.0_f32; GRU_HIDDEN];
    let mut out = [0.0_f32; GRU_HIDDEN];

    for j in 0..GRU_HIDDEN {
        let mut sr = b_r[j];
        let mut sz = b_z[j];
        let mut sn = b_n[j];
        for i in 0..in_dim {
            sr += x[i] * w_ir[j * in_dim + i];
            sz += x[i] * w_iz[j * in_dim + i];
            sn += x[i] * w_in[j * in_dim + i];
        }
        for i in 0..GRU_HIDDEN {
            sr += hidden[i] * w_hr[j * GRU_HIDDEN + i];
            sz += hidden[i] * w_hz[j * GRU_HIDDEN + i];
        }
        r[j] = sigmoid(sr);
        z[j] = sigmoid(sz);
        let mut rh = 0.0_f32;
        for i in 0..GRU_HIDDEN {
            rh += hidden[i] * w_hn[j * GRU_HIDDEN + i];
        }
        n[j] = (sn + r[j] * rh).tanh();
        out[j] = (1.0 - z[j]) * n[j] + z[j] * hidden[j];
    }

    GruCache {
        x: x.to_vec(),
        hidden_prev: *hidden,
        r,
        z,
        n,
        hidden: out,
    }
}

fn gru_backward(
    cache: &GruCache,
    d_h: &[f32; GRU_HIDDEN],
    w_ir: &[f32],
    w_iz: &[f32],
    w_in: &[f32],
    w_hr: &[f32],
    w_hz: &[f32],
    w_hn: &[f32],
    d_x: &mut [f32],
    grads: &mut NetworkGrads,
) {
    let in_dim = cache.x.len();
    let mut d_prev = [0.0_f32; GRU_HIDDEN];

    for j in 0..GRU_HIDDEN {
        let dh = d_h[j];
        let dn = dh * (1.0 - cache.z[j]) * (1.0 - cache.n[j] * cache.n[j]);
        let dz = dh * (cache.hidden_prev[j] - cache.n[j]) * cache.z[j] * (1.0 - cache.z[j]);
        d_prev[j] += dh * cache.z[j];

        grads.gru_b_n[j] += dn;
        for i in 0..in_dim {
            grads.gru_w_in[j * in_dim + i] += dn * cache.x[i];
            d_x[i] += dn * w_in[j * in_dim + i];
        }

        let mut dr = 0.0_f32;
        for i in 0..GRU_HIDDEN {
            dr += dn * cache.hidden_prev[i] * w_hn[j * GRU_HIDDEN + i];
            grads.gru_w_hn[j * GRU_HIDDEN + i] += dn * cache.r[j] * cache.hidden_prev[i];
            d_prev[i] += dn * cache.r[j] * w_hn[j * GRU_HIDDEN + i];
        }
        let dr = dr * cache.r[j] * (1.0 - cache.r[j]);
        grads.gru_b_r[j] += dr;
        for i in 0..in_dim {
            grads.gru_w_ir[j * in_dim + i] += dr * cache.x[i];
            d_x[i] += dr * w_ir[j * in_dim + i];
        }
        for i in 0..GRU_HIDDEN {
            grads.gru_w_hr[j * GRU_HIDDEN + i] += dr * cache.hidden_prev[i];
        }

        grads.gru_b_z[j] += dz;
        for i in 0..in_dim {
            grads.gru_w_iz[j * in_dim + i] += dz * cache.x[i];
            d_x[i] += dz * w_iz[j * in_dim + i];
        }
        for i in 0..GRU_HIDDEN {
            grads.gru_w_hz[j * GRU_HIDDEN + i] += dz * cache.hidden_prev[i];
            d_prev[i] += dz * w_hz[j * GRU_HIDDEN + i];
        }
    }

    let _ = d_prev;
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 as f32) / (u32::MAX as f32) * 2.0 - 1.0
    }

    fn xavier(&mut self, len: usize, fan_in: usize, fan_out: usize) -> Vec<f32> {
        let limit = (6.0 / (fan_in + fan_out) as f32).sqrt();
        (0..len).map(|_| self.next() * limit).collect()
    }
}

pub fn save_to_disk(model: &AimNetwork) -> Result<(), String> {
    let path = super::store::profile_dir()?.join(MODEL_FILE);
    let text = serde_json::to_string(model).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}
