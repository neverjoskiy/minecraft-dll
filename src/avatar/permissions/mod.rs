use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PermissionCategory {
    Movement,
    VanillaModel,
    Nameplate,
    World,
    Render,
    Script,
    Particles,
    Sounds,
    Offscreen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionDecision {
    Allow,
    Deny,
}

#[derive(Clone, Debug)]
pub struct PermissionRule {
    pub category: PermissionCategory,
    pub decision: PermissionDecision,
}

#[derive(Clone, Debug, Default)]
pub struct PermissionPack {
    pub name: String,
    pub rules: HashMap<PermissionCategory, PermissionDecision>,
}

impl PermissionPack {
    pub fn figura_default() -> Self {
        let mut rules = HashMap::new();
        rules.insert(PermissionCategory::Movement, PermissionDecision::Allow);
        rules.insert(PermissionCategory::VanillaModel, PermissionDecision::Allow);
        rules.insert(PermissionCategory::Nameplate, PermissionDecision::Allow);
        rules.insert(PermissionCategory::World, PermissionDecision::Deny);
        rules.insert(PermissionCategory::Render, PermissionDecision::Allow);
        rules.insert(PermissionCategory::Script, PermissionDecision::Allow);
        rules.insert(PermissionCategory::Particles, PermissionDecision::Allow);
        rules.insert(PermissionCategory::Sounds, PermissionDecision::Allow);
        rules.insert(PermissionCategory::Offscreen, PermissionDecision::Deny);
        Self {
            name: "figura_default".to_string(),
            rules,
        }
    }

    pub fn decision(&self, category: PermissionCategory) -> PermissionDecision {
        self.rules
            .get(&category)
            .copied()
            .unwrap_or(PermissionDecision::Deny)
    }

    pub fn set(&mut self, category: PermissionCategory, decision: PermissionDecision) {
        self.rules.insert(category, decision);
    }
}

#[derive(Clone, Debug)]
pub struct TokenBucket {
    pub capacity: f32,
    pub tokens: f32,
    pub refill_per_second: f32,
}

impl TokenBucket {
    pub fn new(capacity: f32, refill_per_second: f32) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_per_second,
        }
    }

    pub fn try_consume(&mut self, amount: f32, delta_seconds: f32) -> bool {
        if !amount.is_finite() || amount < 0.0 || !delta_seconds.is_finite() || delta_seconds < 0.0
        {
            return false;
        }
        self.tokens = (self.tokens + self.refill_per_second * delta_seconds).min(self.capacity);
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComplexityLimits {
    pub instructions: TokenBucket,
    pub complexity: TokenBucket,
}

impl Default for ComplexityLimits {
    fn default() -> Self {
        Self {
            instructions: TokenBucket::new(8192.0, 4096.0),
            complexity: TokenBucket::new(256.0, 128.0),
        }
    }
}

#[derive(Default)]
struct PermissionState {
    pack: PermissionPack,
    limits: ComplexityLimits,
    status: String,
}

fn state() -> &'static Mutex<PermissionState> {
    static STATE: OnceLock<Mutex<PermissionState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(PermissionState {
            pack: PermissionPack::figura_default(),
            limits: ComplexityLimits::default(),
            status: "permissions ready".to_string(),
        })
    })
}

pub fn status() -> String {
    state()
        .lock()
        .map(|guard| guard.status.clone())
        .unwrap_or_else(|_| "permissions poisoned".to_string())
}

pub fn active_pack() -> PermissionPack {
    state()
        .lock()
        .map(|guard| guard.pack.clone())
        .unwrap_or_else(|_| PermissionPack::figura_default())
}

pub fn set_pack(pack: PermissionPack) {
    if let Ok(mut guard) = state().lock() {
        guard.pack = pack;
        guard.status = "permission pack updated".to_string();
    }
}

pub fn check(category: PermissionCategory) -> PermissionDecision {
    state()
        .lock()
        .map(|guard| guard.pack.decision(category))
        .unwrap_or(PermissionDecision::Deny)
}

pub fn try_consume_instruction_budget(amount: f32, delta_seconds: f32) -> bool {
    state()
        .lock()
        .map(|mut guard| {
            let ok = guard.limits.instructions.try_consume(amount, delta_seconds);
            guard.status = if ok {
                "instruction budget consumed".to_string()
            } else {
                "instruction budget exceeded".to_string()
            };
            ok
        })
        .unwrap_or(false)
}

pub fn try_consume_complexity_budget(amount: f32, delta_seconds: f32) -> bool {
    state()
        .lock()
        .map(|mut guard| {
            let ok = guard.limits.complexity.try_consume(amount, delta_seconds);
            guard.status = if ok {
                "complexity budget consumed".to_string()
            } else {
                "complexity budget exceeded".to_string()
            };
            ok
        })
        .unwrap_or(false)
}
