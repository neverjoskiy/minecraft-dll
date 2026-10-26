


#[derive(Clone, Copy, Debug, Default)]
pub struct OutputInterpolator {
    yaw: f64,
    pitch: f64,
    vel_yaw: f64,
    vel_pitch: f64,
    initialized: bool,
}

impl OutputInterpolator {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    
    
    
    
    pub fn step(&mut self, raw_yaw: f64, raw_pitch: f64, error_mag: f64, dt: f64) -> (f64, f64) {
        let dt = dt.clamp(1.0 / 240.0, 0.05);

        if !self.initialized {
            self.yaw = raw_yaw;
            self.pitch = raw_pitch;
            self.vel_yaw = 0.0;
            self.vel_pitch = 0.0;
            self.initialized = true;
            return (raw_yaw, raw_pitch);
        }

        
        let near = (error_mag / 10.0).clamp(0.12, 1.0);
        let stiffness = 14.0 * near;
        let damping = 2.0 * stiffness.sqrt() * 1.25;

        let (y, vy) = spring_step(self.yaw, self.vel_yaw, raw_yaw, stiffness, damping, dt);
        let (p, vp) = spring_step(
            self.pitch,
            self.vel_pitch,
            raw_pitch,
            stiffness,
            damping,
            dt,
        );
        self.yaw = y;
        self.pitch = p;
        self.vel_yaw = vy;
        self.vel_pitch = vp;

        let floor = if error_mag < 1.25 {
            0.014
        } else if error_mag < 3.0 {
            0.007
        } else {
            0.0
        };

        (soft_zero(self.yaw, floor), soft_zero(self.pitch, floor))
    }
}

fn spring_step(
    pos: f64,
    vel: f64,
    target: f64,
    stiffness: f64,
    damping: f64,
    dt: f64,
) -> (f64, f64) {
    let force = (target - pos) * stiffness - vel * damping;
    let vel = vel + force * dt;
    let pos = pos + vel * dt;
    (pos, vel)
}

fn soft_zero(v: f64, floor: f64) -> f64 {
    if floor <= 0.0 {
        return v;
    }
    if v.abs() < floor {
        0.0
    } else {
        v - floor * v.signum()
    }
}
