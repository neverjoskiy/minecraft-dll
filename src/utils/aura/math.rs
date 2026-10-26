
use crate::utils::random;

pub fn random_best(min: f32, max: f32) -> f32 {
    random::range_f64(min as f64, max as f64) as f32
}

pub fn random_i64(min: i64, max: i64) -> i64 {
    min + (random::next_u64() as i64).rem_euclid((max - min).max(1))
}

pub fn wrap_degrees(angle: f32) -> f32 {
    let mut a = angle % 360.0;
    if a >= 180.0 { a -= 360.0; }
    if a < -180.0 { a += 360.0; }
    a
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }

pub fn get_gcd_value() -> f32 {
    
    let sens = 0.5 * 0.6 + 0.2;
    sens * sens * sens * 1.2
}
