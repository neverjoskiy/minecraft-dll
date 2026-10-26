

#[inline]
pub fn lerp(delta: f64, prev: f64, next: f64) -> f64 {
    delta * (next - prev) + prev
}


#[inline]
pub fn catmullrom(delta: f64, prev_a: f64, prev_b: f64, next_a: f64, next_b: f64) -> f64 {
    0.5 * (2.0 * prev_b
        + (-prev_a + next_a) * delta
        + (2.0 * prev_a - 5.0 * prev_b + 4.0 * next_a - next_b) * delta * delta
        + (-prev_a + 3.0 * prev_b - 3.0 * next_a + next_b) * delta * delta * delta)
}

#[inline]
pub fn bezier(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let d = 1.0 - t;
    p0 * d * d * d + 3.0 * p1 * d * d * t + 3.0 * p2 * d * t * t + p3 * t * t * t
}

fn bezier_derivative(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let d = 1.0 - t;
    3.0 * (p1 - p0) * d * d + 6.0 * (p2 - p1) * d * t + 3.0 * (p3 - p2) * t * t
}


pub fn bezier_find_t(x: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let tolerance = 0.0001;
    let mut t = 0.5;
    for _ in 0..100 {
        let x_b = bezier(t, p0, p1, p2, p3);
        let dx = bezier_derivative(t, p0, p1, p2, p3);
        if dx == 0.0 {
            break;
        }
        let next = t - (x_b - x) / dx;
        if (next - t).abs() < tolerance {
            return next;
        }
        t = next;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_midpoint() {
        assert!((lerp(0.5, 0.0, 10.0) - 5.0).abs() < 1e-9);
    }
}
