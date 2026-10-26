

use crate::avatar::permissions::{PermissionCategory, PermissionDecision, check};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum RenderContext {
    ThirdPersonBody,
    FirstPersonHand,
    Armor,
    Cape,
    Elytra,
    HeldItem,
    Skull,
    Projectile,
    Nameplate,
    Shadow,
    Fire,
    Parrot,
    Camera,
    WorldParticle,
    WorldSound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerPolicy {
    
    Replace,
    
    Passthrough,
    
    Suppress,
}

impl RenderContext {
    pub fn permission_category(self) -> PermissionCategory {
        match self {
            Self::ThirdPersonBody
            | Self::FirstPersonHand
            | Self::Armor
            | Self::Cape
            | Self::Elytra
            | Self::HeldItem
            | Self::Skull
            | Self::Projectile
            | Self::Shadow
            | Self::Fire
            | Self::Parrot
            | Self::Camera => PermissionCategory::Render,
            Self::Nameplate => PermissionCategory::Nameplate,
            Self::WorldParticle => PermissionCategory::Particles,
            Self::WorldSound => PermissionCategory::Sounds,
        }
    }

    pub fn touches_vanilla_model(self) -> bool {
        matches!(
            self,
            Self::ThirdPersonBody
                | Self::FirstPersonHand
                | Self::Armor
                | Self::Cape
                | Self::Elytra
                | Self::HeldItem
                | Self::Parrot
        )
    }
}


pub fn layer_policy(context: RenderContext, avatar_wants_replace: bool) -> LayerPolicy {
    if check(context.permission_category()) == PermissionDecision::Deny {
        return LayerPolicy::Passthrough;
    }
    if context.touches_vanilla_model()
        && check(PermissionCategory::VanillaModel) == PermissionDecision::Deny
        && avatar_wants_replace
    {
        
        
        return LayerPolicy::Replace;
    }
    if avatar_wants_replace {
        LayerPolicy::Replace
    } else {
        LayerPolicy::Passthrough
    }
}

pub fn allow_nameplate_override() -> bool {
    check(PermissionCategory::Nameplate) == PermissionDecision::Allow
}

pub fn allow_particles() -> bool {
    check(PermissionCategory::Particles) == PermissionDecision::Allow
}

pub fn allow_sounds() -> bool {
    check(PermissionCategory::Sounds) == PermissionDecision::Allow
}

pub fn allow_camera_offset() -> bool {
    check(PermissionCategory::Movement) == PermissionDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::permissions::{PermissionPack, set_pack};
    use std::sync::{Mutex, OnceLock};

    fn test_gate() -> &'static Mutex<()> {
        static GATE: OnceLock<Mutex<()>> = OnceLock::new();
        GATE.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn default_pack_allows_body_replace() {
        let _guard = test_gate()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        set_pack(PermissionPack::figura_default());
        assert_eq!(
            layer_policy(RenderContext::ThirdPersonBody, true),
            LayerPolicy::Replace
        );
        assert!(allow_nameplate_override());
        assert!(allow_particles());
        assert!(allow_sounds());
    }

    #[test]
    fn denied_render_passthrough() {
        let _guard = test_gate()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pack = PermissionPack::figura_default();
        pack.set(PermissionCategory::Render, PermissionDecision::Deny);
        set_pack(pack);
        assert_eq!(
            layer_policy(RenderContext::FirstPersonHand, true),
            LayerPolicy::Passthrough
        );
        set_pack(PermissionPack::figura_default());
    }
}
