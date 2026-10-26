use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
};

use super::{
    features::{
        FEATURE_DIM, FeatureBuilderInput, FeatureSample, build_features, decompose_delta,
        is_useful_sample, screen_norm_from_error, world_delta_to_view_local,
        world_velocity_to_view_local,
    },
    infer::{SharedInference, shared_inference},
    progress::NeuroProgressPhase,
    recorder::AttackSessionRecorder,
    state::{self, AimAuthority, NeuroState, RotationMode},
    store, train,
};

enum IoJob {
    Append(Vec<FeatureSample>),
    SealAndTrain { max_samples: u64 },
    Reset { max_samples: u64 },
}

pub struct NeuroFrameInput {
    pub yaw_error: f64,
    pub pitch_error: f64,
    pub d_yaw_error: f64,
    pub d_pitch_error: f64,
    pub distance: f64,
    pub rel_x: f64,
    pub rel_y: f64,
    pub rel_z: f64,
    pub target_vel_x: f64,
    pub target_vel_y: f64,
    pub target_vel_z: f64,
    pub view_yaw_degrees: f64,
    pub half_fov: f64,
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
    pub human_d_yaw: f64,
    pub human_d_pitch: f64,
    pub session_break: bool,
    pub has_valid_target: bool,
}

pub struct NeuroTickResult {
    pub authority: AimAuthority,
    pub neuro_delta: Option<(f64, f64)>,
    pub neuro_confidence: f64,
    pub status_line: String,
}

pub struct NeuroController {
    mode: RotationMode,
    state: Arc<AtomicU8>,
    samples: Arc<AtomicU64>,
    max_samples: u64,
    inference: SharedInference,
    recorder: AttackSessionRecorder,
    io_tx: Sender<IoJob>,
    train_busy: Arc<AtomicBool>,
    seal_pending: Arc<AtomicBool>,
    train_dispatched_ms: Arc<AtomicU64>,
    training_started_ms: Arc<AtomicU64>,
    last_error: Arc<Mutex<String>>,
    bootstrapped: bool,
    generation: Arc<AtomicU64>,
}

impl NeuroController {
    pub fn new() -> Self {
        let (io_tx, io_rx) = mpsc::channel::<IoJob>();
        let state = Arc::new(AtomicU8::new(NeuroState::Recording as u8));
        let samples = Arc::new(AtomicU64::new(0));
        let inference = shared_inference();
        let train_busy = Arc::new(AtomicBool::new(false));
        let seal_pending = Arc::new(AtomicBool::new(false));
        let train_dispatched_ms = Arc::new(AtomicU64::new(0));
        let training_started_ms = Arc::new(AtomicU64::new(0));
        let last_error = Arc::new(Mutex::new(String::new()));
        let generation = Arc::new(AtomicU64::new(0));

        let state_w = Arc::clone(&state);
        let samples_w = Arc::clone(&samples);
        let inference_w = Arc::clone(&inference);
        let train_busy_w = Arc::clone(&train_busy);
        let last_error_w = Arc::clone(&last_error);
        let generation_w = Arc::clone(&generation);
        let training_started_w = Arc::clone(&training_started_ms);
        let train_dispatched_w = Arc::clone(&train_dispatched_ms);

        let seal_pending_w = Arc::clone(&seal_pending);

        thread::spawn(move || {
            while let Ok(job) = io_rx.recv() {
                match job {
                    IoJob::Append(batch) => {
                        let clean: Vec<_> = batch.into_iter().filter(is_useful_sample).collect();
                        if clean.is_empty() {
                            continue;
                        }
                        match store::append_samples(&clean) {
                            Ok(count) => samples_w.store(count, Ordering::Release),
                            Err(err) => set_error(&last_error_w, err),
                        }
                    }
                    IoJob::SealAndTrain { max_samples } => {
                        let job_gen = generation_w.load(Ordering::Acquire);
                        run_training_job(
                            &state_w,
                            &inference_w,
                            &last_error_w,
                            &generation_w,
                            job_gen,
                            &train_busy_w,
                            &training_started_w,
                            &train_dispatched_w,
                            max_samples,
                        );
                    }
                    IoJob::Reset { max_samples } => {
                        generation_w.fetch_add(1, Ordering::AcqRel);
                        let job_gen = generation_w.load(Ordering::Acquire);
                        inference_w.clear();
                        clear_error(&last_error_w);

                        let mut manifest = store::load_manifest(max_samples);
                        manifest.model_ready = false;
                        manifest.max_samples = max_samples;
                        manifest.last_error.clear();
                        let _ = store::save_manifest(&manifest);
                        samples_w.store(manifest.samples_count, Ordering::Release);

                        finish_training_job(
                            &train_busy_w,
                            &training_started_w,
                            &train_dispatched_w,
                        );
                        seal_pending_w.store(false, Ordering::Release);

                        if manifest.samples_count >= max_samples {
                            train_busy_w.store(true, Ordering::Release);
                            seal_pending_w.store(true, Ordering::Release);
                            train_dispatched_w.store(unix_now_ms(), Ordering::Release);
                            run_training_job(
                                &state_w,
                                &inference_w,
                                &last_error_w,
                                &generation_w,
                                job_gen,
                                &train_busy_w,
                                &training_started_w,
                                &train_dispatched_w,
                                max_samples,
                            );
                        } else {
                            state::store_state(&state_w, NeuroState::Recording);
                        }
                    }
                }
            }
        });

        Self {
            mode: RotationMode::Default,
            state,
            samples,
            max_samples: 20_000,
            inference,
            recorder: AttackSessionRecorder::new(),
            io_tx,
            train_busy,
            seal_pending,
            train_dispatched_ms,
            training_started_ms,
            last_error,
            bootstrapped: false,
            generation,
        }
    }

    pub fn configure(&mut self, mode: RotationMode, max_samples: u64, reset_requested: bool) {
        let prev_max = self.max_samples;
        self.mode = mode;
        self.max_samples = max_samples.max(2_000);

        if !self.bootstrapped {
            self.bootstrap();
        } else if self.max_samples != prev_max {
            self.sync_manifest_max_samples();
            if self.samples_count() < self.max_samples {
                self.seal_pending.store(false, Ordering::Release);
            }
        }

        if reset_requested {
            self.seal_pending.store(false, Ordering::Release);
            self.request_reset();
        }
    }

    pub fn reset_inference_state(&self) {
        self.inference.reset_state();
    }

    fn bootstrap(&mut self) {
        let manifest = store::load_manifest(self.max_samples);
        let layout_ok = manifest.feature_layout_hash == super::features::FEATURE_LAYOUT_HASH;

        if !layout_ok && (manifest.samples_count > 0 || store::model_exists()) {
            let _ = store::reset_profile(self.max_samples);
            self.samples.store(0, Ordering::Release);
            self.inference.clear();
            state::store_state(&self.state, NeuroState::Recording);
            clear_error(&self.last_error);
            self.bootstrapped = true;
            return;
        }

        self.samples
            .store(manifest.samples_count, Ordering::Release);

        if store::model_exists() && layout_ok {
            match self.inference.load_from_disk() {
                Ok(()) => {
                    state::store_state(&self.state, NeuroState::Ready);
                    clear_error(&self.last_error);
                }
                Err(err) => {
                    let _ = store::remove_model_files();
                    set_error(&self.last_error, err);
                    self.recover_without_model(manifest.samples_count);
                }
            }
        } else {
            self.recover_without_model(manifest.samples_count);
        }

        self.bootstrapped = true;
    }

    fn recover_without_model(&self, samples_count: u64) {
        if samples_count >= self.max_samples {
            self.try_start_training();
        } else {
            state::store_state(&self.state, NeuroState::Recording);
        }
    }

    fn try_start_training(&self) -> bool {
        if self.seal_pending.load(Ordering::Acquire) {
            return false;
        }
        if self.train_busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.seal_pending.store(true, Ordering::Release);
        self.train_dispatched_ms
            .store(unix_now_ms(), Ordering::Release);
        if self
            .io_tx
            .send(IoJob::SealAndTrain {
                max_samples: self.max_samples,
            })
            .is_err()
        {
            self.train_busy.store(false, Ordering::Release);
            self.seal_pending.store(false, Ordering::Release);
            self.train_dispatched_ms.store(0, Ordering::Release);
            set_error(
                &self.last_error,
                "neuro io worker unavailable; restart client".into(),
            );
            return false;
        }
        true
    }

    fn recover_stale_training(&self) {
        if self.neuro_state() != NeuroState::Training {
            return;
        }

        if self.train_busy.load(Ordering::Acquire) {
            return;
        }

        if store::model_exists() {
            match self.inference.load_from_disk() {
                Ok(()) => {
                    state::store_state(&self.state, NeuroState::Ready);
                    clear_error(&self.last_error);
                }
                Err(err) => {
                    set_error(&self.last_error, err);
                    state::store_state(&self.state, NeuroState::Recording);
                }
            }
        } else {
            set_error(
                &self.last_error,
                "training failed; press Reset neuro to retry".into(),
            );
            state::store_state(&self.state, NeuroState::Recording);
        }

        self.train_dispatched_ms.store(0, Ordering::Release);
        self.training_started_ms.store(0, Ordering::Release);
    }

    fn sync_manifest_max_samples(&self) {
        let mut manifest = store::load_manifest(self.max_samples);
        manifest.max_samples = self.max_samples;
        let _ = store::save_manifest(&manifest);
    }

    fn request_reset(&mut self) {
        self.recorder.clear();
        self.seal_pending.store(false, Ordering::Release);
        let _ = self.io_tx.send(IoJob::Reset {
            max_samples: self.max_samples,
        });
    }

    pub fn neuro_state(&self) -> NeuroState {
        state::load_state(&self.state)
    }

    pub fn samples_count(&self) -> u64 {
        self.samples.load(Ordering::Acquire)
    }

    pub fn max_samples(&self) -> u64 {
        self.max_samples
    }

    pub fn publish_progress_overlay(&self) {
        if self.mode != RotationMode::Neuro {
            super::progress::clear_progress_overlay();
            return;
        }

        let neuro_state = self.neuro_state();
        let visible = matches!(neuro_state, NeuroState::Recording | NeuroState::Training);
        let phase = match neuro_state {
            NeuroState::Training => NeuroProgressPhase::Training,
            _ => NeuroProgressPhase::Recording,
        };
        let samples = self.samples_count().min(self.max_samples);
        let detail = self.training_overlay_detail();
        super::progress::set_progress_overlay(super::progress::NeuroProgressOverlay {
            visible,
            phase,
            samples,
            max_samples: self.max_samples,
            detail,
        });
    }

    pub fn tick(&mut self, input: NeuroFrameInput) -> NeuroTickResult {
        if self.mode != RotationMode::Neuro {
            return NeuroTickResult {
                authority: AimAuthority::ClassicAssist,
                neuro_delta: None,
                neuro_confidence: 1.0,
                status_line: "Mode: Default".into(),
            };
        }

        let neuro_state = self.neuro_state();
        let samples = self.samples.load(Ordering::Acquire);
        let err = self
            .last_error
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        self.recover_stale_training();

        let neuro_state = self.neuro_state();

        match neuro_state {
            NeuroState::Recording => {
                if input.has_valid_target {
                    let decomposed = decompose_delta(
                        input.human_d_yaw,
                        input.human_d_pitch,
                        input.yaw_error,
                        input.pitch_error,
                    );
                    let tracking = decomposed.is_some();

                    if let Some((labels, _weight)) = decomposed {
                        let sample = self.build_sample(&input, labels);
                        if let Some(session) =
                            self.recorder.push_frame(sample, input.session_break, true)
                        {
                            let _ = self.io_tx.send(IoJob::Append(session));
                        }
                    } else if input.session_break {
                        if let Some(session) = self.recorder.flush_active() {
                            let _ = self.io_tx.send(IoJob::Append(session));
                        }
                    } else if !tracking {
                        let sample = self.build_sample(&input, [0.0; 4]);
                        let _ = self.recorder.push_frame(sample, false, false);
                    }
                } else if let Some(session) = self.recorder.flush_active() {
                    let _ = self.io_tx.send(IoJob::Append(session));
                }

                let current = self.samples.load(Ordering::Acquire);
                if current >= self.max_samples
                    && !self.seal_pending.load(Ordering::Acquire)
                    && !self.train_busy.load(Ordering::Acquire)
                {
                    if let Some(session) = self.recorder.flush_active() {
                        let _ = self.io_tx.send(IoJob::Append(session));
                    }
                    self.try_start_training();
                }

                NeuroTickResult {
                    authority: AimAuthority::ManualOnly,
                    neuro_delta: None,
                    neuro_confidence: 0.0,
                    status_line: format!(
                        "Neuro Recording {}/{}{}",
                        current,
                        self.max_samples,
                        format_err(&err)
                    ),
                }
            }
            NeuroState::Training => NeuroTickResult {
                authority: AimAuthority::ManualOnly,
                neuro_delta: None,
                neuro_confidence: 0.0,
                status_line: format!("Neuro Training...{}", format_err(&err)),
            },
            NeuroState::Ready => {
                let neuro_delta = if input.has_valid_target {
                    let features = self.build_features(&input);
                    self.inference
                        .predict(features)
                        .map(|(yaw, pitch, confidence)| (Some((yaw, pitch)), confidence))
                } else {
                    None
                };

                let (delta, confidence) = match neuro_delta {
                    Some((Some(d), c)) => (Some(d), c),
                    _ => (None, 0.0),
                };

                NeuroTickResult {
                    authority: AimAuthority::NeuroAssist,
                    neuro_delta: delta,
                    neuro_confidence: confidence,
                    status_line: format!("Neuro Ready (n={samples}){}", format_err(&err)),
                }
            }
        }
    }

    fn training_overlay_detail(&self) -> String {
        if self.neuro_state() == NeuroState::Training {
            let (epoch, total) = train::training_progress();
            if total > 0 {
                return format!("epoch {epoch}/{total}");
            }
            return "preparing dataset...".into();
        }

        self.last_error
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn build_features(&self, input: &NeuroFrameInput) -> [f32; FEATURE_DIM] {
        let (local_right, local_up, local_forward) = world_delta_to_view_local(
            input.rel_x,
            input.rel_y,
            input.rel_z,
            input.view_yaw_degrees,
        );
        let (target_vel_right, target_vel_forward, target_vel_up) = world_velocity_to_view_local(
            input.target_vel_x,
            input.target_vel_y,
            input.target_vel_z,
            input.view_yaw_degrees,
        );
        let (screen_x, screen_y) =
            screen_norm_from_error(input.yaw_error, input.pitch_error, input.half_fov);

        build_features(FeatureBuilderInput {
            yaw_error: input.yaw_error,
            pitch_error: input.pitch_error,
            d_yaw_error: input.d_yaw_error,
            d_pitch_error: input.d_pitch_error,
            distance: input.distance,
            local_right,
            local_up,
            local_forward,
            target_vel_right,
            target_vel_forward,
            target_vel_up,
            screen_x_norm: screen_x,
            screen_y_norm: screen_y,
            mouse_step: input.mouse_step,
            dt: input.dt,
            frames_since_acquire: input.frames_since_acquire,
            time_since_attack: input.time_since_attack,
            attack_held: input.attack_held,
            on_ground: input.on_ground,
            sprint: input.sprint,
            sneak: input.sneak,
            fall_distance: input.fall_distance,
            attack_strength: input.attack_strength,
            target_health: input.target_health,
            prev_yaw_delta: input.prev_yaw_delta,
            prev_pitch_delta: input.prev_pitch_delta,
            accel_yaw_delta: input.accel_yaw_delta,
            accel_pitch_delta: input.accel_pitch_delta,
        })
    }

    fn build_sample(&self, input: &NeuroFrameInput, labels: [f32; 4]) -> FeatureSample {
        FeatureSample {
            features: self.build_features(input),
            labels,
            session_end: false,
        }
    }
}

fn set_error(slot: &Mutex<String>, message: String) {
    if let Ok(mut guard) = slot.lock() {
        *guard = message;
    }
}

fn clear_error(slot: &Mutex<String>) {
    if let Ok(mut guard) = slot.lock() {
        guard.clear();
    }
}

fn format_err(err: &str) -> String {
    if err.is_empty() {
        String::new()
    } else {
        format!(" | err: {err}")
    }
}

fn run_training_job(
    state_w: &Arc<AtomicU8>,
    inference_w: &SharedInference,
    last_error_w: &Arc<Mutex<String>>,
    generation_w: &Arc<AtomicU64>,
    job_gen: u64,
    train_busy_w: &Arc<AtomicBool>,
    training_started_w: &Arc<AtomicU64>,
    train_dispatched_w: &Arc<AtomicU64>,
    max_samples: u64,
) {
    let _ = store::remove_model_files();
    training_started_w.store(unix_now_ms(), Ordering::Release);
    clear_error(last_error_w);
    state::store_state(state_w, NeuroState::Training);

    let result = std::panic::catch_unwind(|| train::train_from_dataset(0, 0, 0.0));

    if generation_w.load(Ordering::Acquire) != job_gen {
        finish_training_job(train_busy_w, training_started_w, train_dispatched_w);
        state::store_state(state_w, NeuroState::Recording);
        return;
    }

    match result {
        Ok(Ok(_train_result)) => match inference_w.load_from_disk() {
            Ok(()) => {
                let mut manifest = store::load_manifest(max_samples);
                manifest.model_ready = true;
                manifest.max_samples = max_samples;
                manifest.last_error.clear();
                let _ = store::save_manifest(&manifest);
                state::store_state(state_w, NeuroState::Ready);
                clear_error(last_error_w);
            }
            Err(err) => {
                set_error(last_error_w, err);
                state::store_state(state_w, NeuroState::Recording);
            }
        },
        Ok(Err(err)) => {
            set_error(last_error_w, err);
            state::store_state(state_w, NeuroState::Recording);
        }
        Err(_) => {
            set_error(last_error_w, "training panicked; press Reset neuro".into());
            state::store_state(state_w, NeuroState::Recording);
        }
    }

    finish_training_job(train_busy_w, training_started_w, train_dispatched_w);
}

fn finish_training_job(
    train_busy: &AtomicBool,
    training_started_ms: &AtomicU64,
    train_dispatched_ms: &AtomicU64,
) {
    train_busy.store(false, Ordering::Release);
    training_started_ms.store(0, Ordering::Release);
    train_dispatched_ms.store(0, Ordering::Release);
    train::clear_training_progress();
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
