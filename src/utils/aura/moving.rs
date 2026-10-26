
pub fn direction(rotation_yaw: f32, move_forward: f32, move_strafing: f32) -> f64 {
    let mut yaw = rotation_yaw;
    if move_forward < 0.0 { yaw += 180.0; }
    let mut forward = 1.0_f32;
    if move_forward < 0.0 { forward = -0.5; }
    if move_forward > 0.0 { forward = 0.5; }
    if move_strafing > 0.0 { yaw -= 90.0 * forward; }
    if move_strafing < 0.0 { yaw += 90.0 * forward; }
    (yaw as f64).to_radians()
}

pub fn wrap_degrees(angle: f32) -> f32 { crate::utils::aura::math::wrap_degrees(angle) }


pub fn fix_movement_free(free_yaw: f32, player_yaw: f32, forward: f32, strafe: f32) -> (f32, f32) {
    if forward == 0.0 && strafe == 0.0 { return (forward, strafe); }
    let angle = crate::utils::aura::math::wrap_degrees(direction(free_yaw, forward, strafe).to_degrees() as f32);
    let mut closest_forward = 0.0_f32;
    let mut closest_strafe = 0.0_f32;
    let mut closest_diff = f32::MAX;
    let mut pf = -1.0_f32;
    while pf <= 1.0 {
        let mut ps = -1.0_f32;
        while ps <= 1.0 {
            if ps != 0.0 || pf != 0.0 {
                let predicted = crate::utils::aura::math::wrap_degrees(direction(player_yaw, pf, ps).to_degrees() as f32);
                let diff = crate::utils::aura::math::wrap_degrees(angle - predicted).abs();
                if diff < closest_diff { closest_diff = diff; closest_forward = pf; closest_strafe = ps; }
            }
            ps += 1.0;
        }
        pf += 1.0;
    }
    (closest_forward, closest_strafe)
}


pub fn fix_movement_focus(yaw: f32, forward: f32, strafe: f32) -> (f32, f32) {
    if forward == 0.0 && strafe == 0.0 { return (forward, strafe); }
    let target_angle = crate::utils::aura::math::wrap_degrees(direction(yaw, forward, strafe).to_degrees() as f32);
    let mut best_forward = 0.0_f32;
    let mut best_strafe = 0.0_f32;
    let mut smallest = f32::MAX;
    let mut tf = -1.0_f32;
    while tf <= 1.0 {
        let mut ts = -1.0_f32;
        while ts <= 1.0 {
            if tf != 0.0 || ts != 0.0 {
                let test_angle = crate::utils::aura::math::wrap_degrees(direction(yaw, tf, ts).to_degrees() as f32);
                let diff = crate::utils::aura::math::wrap_degrees(target_angle - test_angle).abs();
                if diff < smallest { smallest = diff; best_forward = tf; best_strafe = ts; }
            }
            ts += 1.0;
        }
        tf += 1.0;
    }
    (best_forward, best_strafe)
}
