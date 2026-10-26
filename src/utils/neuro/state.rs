use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotationMode {
    Default,
    Neuro,
}

impl RotationMode {
    pub fn from_choice(value: &str) -> Self {
        if value.eq_ignore_ascii_case("Neuro") {
            Self::Neuro
        } else {
            Self::Default
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Neuro => "Neuro",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NeuroState {
    Recording = 0,
    Training = 1,
    Ready = 2,
}

impl NeuroState {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Training,
            2 => Self::Ready,
            _ => Self::Recording,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "Recording",
            Self::Training => "Training",
            Self::Ready => "Ready",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AimAuthority {
    ManualOnly,
    ClassicAssist,
    NeuroAssist,
}

pub fn load_state(atomic: &AtomicU8) -> NeuroState {
    NeuroState::from_u8(atomic.load(Ordering::Acquire))
}

pub fn store_state(atomic: &AtomicU8, state: NeuroState) {
    atomic.store(state as u8, Ordering::Release);
}
