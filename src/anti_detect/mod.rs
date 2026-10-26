




mod injgen;
mod jvm_image;

pub use injgen::{NeutralizeReport, neutralize, neutralize_periodic};

use std::time::Duration;


pub fn install() {
    let _ = neutralize();
}

pub fn on_frame() {
    let _ = neutralize_periodic(Duration::from_secs(2));
}
