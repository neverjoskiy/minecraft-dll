mod controller;
mod features;
mod infer;
mod linear_model;
mod network;
mod output_filter;
mod progress;
mod recorder;
mod state;
mod store;
mod train;
mod trainer;
pub use controller::{NeuroController, NeuroFrameInput};
pub use features::{
    FEATURE_DIM, FeatureSample, LABEL_DIM, SEQ_LEN, blend_with_classic, clamp_delta_toward_error,
};
pub use output_filter::OutputInterpolator;
pub use progress::{
    NeuroProgressOverlay, NeuroProgressPhase, clear_progress_overlay, progress_overlay,
};
pub use state::{AimAuthority, NeuroState, RotationMode};
