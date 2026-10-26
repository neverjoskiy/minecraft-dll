use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static RNG_STATE: AtomicU64 = AtomicU64::new(0);

pub fn f32() -> f32 {
    let value = next_u64() >> 40;
    value as f32 / ((1_u64 << 24) as f32)
}

pub fn f64() -> f64 {
    let value = next_u64() >> 11;
    value as f64 / ((1_u64 << 53) as f64)
}

pub fn range_f64(min: f64, max: f64) -> f64 {
    if min >= max {
        return min;
    }

    min + (max - min) * f64()
}

pub fn range_u64(max_exclusive: u64) -> u64 {
    if max_exclusive == 0 {
        return 0;
    }

    next_u64() % max_exclusive
}

pub fn range_u64_between(min_inclusive: u64, max_exclusive: u64) -> u64 {
    if min_inclusive >= max_exclusive {
        return min_inclusive;
    }

    min_inclusive + range_u64(max_exclusive - min_inclusive)
}

pub fn chance_percent(percent: f32) -> bool {
    let percent = percent.clamp(0.0, 100.0);
    f32() * 100.0 < percent
}

pub fn next_u64() -> u64 {
    loop {
        let current = RNG_STATE.load(Ordering::Acquire);
        let state = if current == 0 { seed() } else { current };
        let mut next = state;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        if next == 0 {
            next = seed();
        }

        match RNG_STATE.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return next,
            Err(_) => continue,
        }
    }
}

fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    nanos ^ 0xA076_1D64_78BD_642F
}
