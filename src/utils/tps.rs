use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::utils::time;

static LAST_GAME_TIME: AtomicI64 = AtomicI64::new(-1);
static LAST_SAMPLE_MS: AtomicU64 = AtomicU64::new(0);
static TICK_INTERVAL_MS: AtomicU64 = AtomicU64::new(50);

pub fn sample_game_time(game_time: i64) {
    let now = time::now_ms();
    let last_game_time = LAST_GAME_TIME.load(Ordering::Acquire);
    if last_game_time == game_time {
        return;
    }

    let last_sample = LAST_SAMPLE_MS.load(Ordering::Acquire);
    if last_game_time >= 0 && last_sample != 0 && game_time > last_game_time {
        let elapsed = now.saturating_sub(last_sample);
        let ticks = (game_time - last_game_time) as u64;
        if ticks > 0 {
            let measured = (elapsed / ticks).clamp(50, 150);
            let previous = TICK_INTERVAL_MS.load(Ordering::Acquire);
            let smoothed = ((previous * 3) + measured) / 4;
            TICK_INTERVAL_MS.store(smoothed, Ordering::Release);
        }
    }

    LAST_GAME_TIME.store(game_time, Ordering::Release);
    LAST_SAMPLE_MS.store(now, Ordering::Release);
}

pub fn tick_multiplier() -> f32 {
    (TICK_INTERVAL_MS.load(Ordering::Acquire) as f32 / 50.0).clamp(1.0, 3.0)
}

pub fn adjust_delay(delay_ms: u64) -> u64 {
    delay_ms
}
