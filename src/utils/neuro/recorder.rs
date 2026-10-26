use std::collections::VecDeque;

use super::features::FeatureSample;

const PRE_ROLL_CAP: usize = 64;
const SESSION_MAX_FRAMES: usize = 240;

#[derive(Default)]
pub struct AttackSessionRecorder {
    ring: VecDeque<FeatureSample>,
    active: Option<Vec<FeatureSample>>,
    frames_in_session: usize,
}

impl AttackSessionRecorder {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(PRE_ROLL_CAP),
            active: None,
            frames_in_session: 0,
        }
    }

    pub fn clear(&mut self) {
        self.ring.clear();
        self.active = None;
        self.frames_in_session = 0;
    }

    
    
    pub fn push_frame(
        &mut self,
        sample: FeatureSample,
        session_break: bool,
        tracking: bool,
    ) -> Option<Vec<FeatureSample>> {
        self.ring.push_back(sample);
        if self.ring.len() > PRE_ROLL_CAP {
            self.ring.pop_front();
        }

        if session_break {
            let closed = self.active.take().filter(|session| session.len() >= 4);
            let mut session: Vec<FeatureSample> = self.ring.iter().copied().collect();
            self.frames_in_session = session.len();
            self.active = Some(session);
            return closed;
        }

        if tracking {
            if self.active.is_none() {
                let session: Vec<FeatureSample> = self.ring.iter().copied().collect();
                self.frames_in_session = session.len();
                self.active = Some(session);
            } else if let Some(session) = self.active.as_mut() {
                session.push(sample);
                self.frames_in_session = self.frames_in_session.saturating_add(1);
            }
        }

        if let Some(session) = self.active.as_mut() {
            if self.frames_in_session >= SESSION_MAX_FRAMES {
                let mut closed = self.active.take().unwrap_or_default();
                if !closed.is_empty() {
                    if let Some(last) = closed.last_mut() {
                        last.session_end = true;
                    }
                }
                self.frames_in_session = 0;
                return Some(closed).filter(|s| s.len() >= 4);
            }
        }

        None
    }

    pub fn flush_active(&mut self) -> Option<Vec<FeatureSample>> {
        self.frames_in_session = 0;
        let mut closed = self.active.take()?;
        if closed.len() < 4 {
            return None;
        }
        if let Some(last) = closed.last_mut() {
            last.session_end = true;
        }
        Some(closed)
    }
}
