use crate::avatar::animation::Keyframe;
use crate::avatar::animation::TransformType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
    Holding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Once,
    Loop,
    Hold,
}

impl LoopMode {
    pub fn from_bb(s: Option<&str>) -> Self {
        match s.map(|v| v.to_ascii_lowercase()).as_deref() {
            Some("loop") => Self::Loop,
            Some("hold") => Self::Hold,
            _ => Self::Once,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnimationChannel {
    pub transform: TransformType,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Debug)]
pub struct PartChannels {
    pub part_name: String,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub model_name: String,
    pub name: String,
    pub length: f32,
    pub blend: f32,
    pub offset: f32,
    pub speed: f32,
    pub start_delay: f32,
    pub loop_delay: f32,
    pub priority: i32,
    pub override_mask: u8,
    pub loop_mode: LoopMode,
    pub play_state: PlayState,
    pub time: f32,
    pub frame_time: f32,
    pub last_time: f32,
    pub inverted: bool,
    pub parts: Vec<PartChannels>,
}

impl Animation {
    pub fn new(model_name: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            name: name.into(),
            length: 0.0,
            blend: 1.0,
            offset: 0.0,
            speed: 1.0,
            start_delay: 0.0,
            loop_delay: 0.0,
            priority: 0,
            override_mask: 0,
            loop_mode: LoopMode::Once,
            play_state: PlayState::Stopped,
            time: 0.0,
            frame_time: 0.0,
            last_time: 0.0,
            inverted: false,
            parts: Vec::new(),
        }
    }

    pub fn play(&mut self) {
        if self.play_state == PlayState::Stopped {
            self.time = if self.inverted {
                self.length
            } else {
                self.offset
            };
        }
        self.play_state = PlayState::Playing;
    }

    pub fn stop(&mut self) {
        self.play_state = PlayState::Stopped;
        self.time = self.offset;
        self.frame_time = self.offset;
    }

    pub fn pause(&mut self) {
        if self.play_state == PlayState::Playing {
            self.play_state = PlayState::Paused;
        }
    }

    pub fn is_playing(&self) -> bool {
        self.play_state == PlayState::Playing
    }

    pub fn tick(&mut self, dt: f32) {
        if self.play_state != PlayState::Playing {
            return;
        }
        if !dt.is_finite() || dt < 0.0 || !self.speed.is_finite() {
            return;
        }
        let step = dt * self.speed.abs();
        self.time += if self.inverted { -step } else { step };
        match self.loop_mode {
            LoopMode::Once => {
                if (!self.inverted && self.time >= self.length)
                    || (self.inverted && self.time <= self.offset)
                {
                    self.time = if self.inverted {
                        self.offset
                    } else {
                        self.length
                    };
                    self.play_state = PlayState::Stopped;
                }
            }
            LoopMode::Loop => {
                let span = self.length + self.loop_delay - self.offset;
                if span <= 0.0 || !span.is_finite() {
                    self.time = self.offset;
                } else if !self.inverted && self.time > self.length + self.loop_delay {
                    self.time = self.offset + (self.time - self.offset).rem_euclid(span);
                } else if self.inverted && self.time < self.offset - self.loop_delay {
                    self.time = self.length - (self.length - self.time).rem_euclid(span);
                }
            }
            LoopMode::Hold => {
                self.time = if self.inverted {
                    self.time.max(self.offset)
                } else {
                    self.time.min(self.length)
                };
                if (!self.inverted && self.time >= self.length)
                    || (self.inverted && self.time <= self.offset)
                {
                    self.play_state = PlayState::Holding;
                }
            }
        }
        self.last_time = self.frame_time;
        self.frame_time = self.time.max(self.offset);
    }

    pub fn override_rot(&self) -> bool {
        self.override_mask & 1 != 0
    }
    pub fn override_pos(&self) -> bool {
        self.override_mask & 2 != 0
    }
    pub fn override_scale(&self) -> bool {
        self.override_mask & 4 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_hold_finishes_at_offset() {
        let mut animation = Animation::new("model", "reverse");
        animation.length = 2.0;
        animation.offset = 0.5;
        animation.inverted = true;
        animation.loop_mode = LoopMode::Hold;
        animation.play();

        animation.tick(2.0);

        assert_eq!(animation.play_state, PlayState::Holding);
        assert_eq!(animation.frame_time, 0.5);
    }

    #[test]
    fn zero_length_loop_stays_at_offset() {
        let mut animation = Animation::new("model", "static");
        animation.loop_mode = LoopMode::Loop;
        animation.play();

        animation.tick(1.0);

        assert_eq!(animation.time, 0.0);
        assert_eq!(animation.frame_time, 0.0);
    }
}
