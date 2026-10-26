use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TimerUtils {
    millis: Instant,
}

impl Default for TimerUtils {
    fn default() -> Self {
        Self { millis: Instant::now() }
    }
}

impl TimerUtils {
    pub fn new() -> Self { Self::default() }
    pub fn finished(&self, delay_ms: u64) -> bool {
        self.millis.elapsed() >= Duration::from_millis(delay_ms)
    }
    pub fn finished_f32(&self, delay_ms: f32) -> bool {
        self.millis.elapsed().as_secs_f32() * 1000.0 >= delay_ms
    }
    pub fn reset(&mut self) { self.millis = Instant::now(); }
    pub fn get_elapsed_time(&self) -> u64 { self.millis.elapsed().as_millis() as u64 }
    pub fn get_millis(&self) -> u64 { self.millis.elapsed().as_millis() as u64 }
}
