

use crate::avatar::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParentType {
    None,
    Head,
    Body,
    LeftArm,
    RightArm,
    LeftLeggingPivot,
    RightLeggingPivot,
    LeftLeg,
    RightLeg,
    LeftElytraPivot,
    RightElytraPivot,
    LeftElytra,
    RightElytra,
    Cape,
    World,
    Hud,
    Camera,
    Skull,
    Portrait,
    Arrow,
    Trident,
    Item,
    LeftItemPivot,
    RightItemPivot,
    LeftSpyglassPivot,
    RightSpyglassPivot,
    LeftParrotPivot,
    RightParrotPivot,
    HelmetItemPivot,
    HelmetPivot,
    ChestplatePivot,
    LeftShoulderPivot,
    RightShoulderPivot,
    LeggingsPivot,
    LeftBootPivot,
    RightBootPivot,
}

impl ParentType {
    pub fn offset(self) -> Vec3 {
        match self {
            Self::LeftArm => Vec3::new(5.0, 2.0, 0.0),
            Self::RightArm => Vec3::new(-5.0, 2.0, 0.0),
            Self::LeftLeg => Vec3::new(1.9, 12.0, 0.0),
            Self::RightLeg => Vec3::new(-1.9, 12.0, 0.0),
            Self::LeftElytra => Vec3::new(5.0, 0.0, 0.0),
            Self::RightElytra => Vec3::new(-5.0, 0.0, 0.0),
            _ => Vec3::zero(),
        }
    }

    pub fn is_separate(self) -> bool {
        matches!(
            self,
            Self::LeftElytra
                | Self::RightElytra
                | Self::Cape
                | Self::World
                | Self::Hud
                | Self::Skull
                | Self::Portrait
                | Self::Arrow
                | Self::Trident
                | Self::Item
        )
    }

    pub fn is_pivot(self) -> bool {
        matches!(
            self,
            Self::LeftLeggingPivot
                | Self::RightLeggingPivot
                | Self::LeftElytraPivot
                | Self::RightElytraPivot
                | Self::LeftItemPivot
                | Self::RightItemPivot
                | Self::LeftSpyglassPivot
                | Self::RightSpyglassPivot
                | Self::LeftParrotPivot
                | Self::RightParrotPivot
                | Self::HelmetItemPivot
                | Self::HelmetPivot
                | Self::ChestplatePivot
                | Self::LeftShoulderPivot
                | Self::RightShoulderPivot
                | Self::LeggingsPivot
                | Self::LeftBootPivot
                | Self::RightBootPivot
        )
    }

    
    
    
    
    
    pub fn from_name(name: &str) -> Self {
        if name.is_empty() {
            return Self::None;
        }
        for (ty, aliases) in ALIASES {
            if name == ty.figura_name() || aliases.iter().any(|a| *a == name) {
                return *ty;
            }
        }
        let mut best: Option<(usize, ParentType)> = None;
        for (ty, aliases) in ALIASES {
            let enum_name = ty.figura_name();
            if name.starts_with(enum_name) {
                let len = enum_name.len();
                if best.map(|(n, _)| len > n).unwrap_or(true) {
                    best = Some((len, *ty));
                }
            }
            for alias in *aliases {
                if name.starts_with(alias) {
                    let len = alias.len();
                    if best.map(|(n, _)| len > n).unwrap_or(true) {
                        best = Some((len, *ty));
                    }
                }
            }
        }
        best.map(|(_, ty)| ty).unwrap_or(Self::None)
    }

    
    fn figura_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Head => "Head",
            Self::Body => "Body",
            Self::LeftArm => "LeftArm",
            Self::RightArm => "RightArm",
            Self::LeftLeggingPivot => "LeftLeggingPivot",
            Self::RightLeggingPivot => "RightLeggingPivot",
            Self::LeftLeg => "LeftLeg",
            Self::RightLeg => "RightLeg",
            Self::LeftElytraPivot => "LeftElytraPivot",
            Self::RightElytraPivot => "RightElytraPivot",
            Self::LeftElytra => "LeftElytra",
            Self::RightElytra => "RightElytra",
            Self::Cape => "Cape",
            Self::World => "World",
            Self::Hud => "Hud",
            Self::Camera => "Camera",
            Self::Skull => "Skull",
            Self::Portrait => "Portrait",
            Self::Arrow => "Arrow",
            Self::Trident => "Trident",
            Self::Item => "Item",
            Self::LeftItemPivot => "LeftItemPivot",
            Self::RightItemPivot => "RightItemPivot",
            Self::LeftSpyglassPivot => "LeftSpyglassPivot",
            Self::RightSpyglassPivot => "RightSpyglassPivot",
            Self::LeftParrotPivot => "LeftParrotPivot",
            Self::RightParrotPivot => "RightParrotPivot",
            Self::HelmetItemPivot => "HelmetItemPivot",
            Self::HelmetPivot => "HelmetPivot",
            Self::ChestplatePivot => "ChestplatePivot",
            Self::LeftShoulderPivot => "LeftShoulderPivot",
            Self::RightShoulderPivot => "RightShoulderPivot",
            Self::LeggingsPivot => "LeggingsPivot",
            Self::LeftBootPivot => "LeftBootPivot",
            Self::RightBootPivot => "RightBootPivot",
        }
    }
}

const ALIASES: &[(ParentType, &[&str])] = &[
    (ParentType::None, &["NONE", "Model", "MODEL"]),
    (ParentType::Head, &["Head", "HEAD"]),
    (ParentType::Body, &["Body", "BODY"]),
    (ParentType::LeftArm, &["LeftArm", "LEFT_ARM"]),
    (ParentType::RightArm, &["RightArm", "RIGHT_ARM"]),
    (
        ParentType::LeftLeggingPivot,
        &["LEFT_LEGGING_PIVOT", "LeftLeggingPivot"],
    ),
    (
        ParentType::RightLeggingPivot,
        &["RIGHT_LEGGING_PIVOT", "RightLeggingPivot"],
    ),
    (ParentType::LeftLeg, &["LeftLeg", "LEFT_LEG"]),
    (ParentType::RightLeg, &["RightLeg", "RIGHT_LEG"]),
    (
        ParentType::LeftElytraPivot,
        &[
            "LEFT_ELYTRA_PIVOT",
            "LeftElytraPivot",
            "LeftWingPivot",
            "LEFT_WING_PIVOT",
        ],
    ),
    (
        ParentType::RightElytraPivot,
        &[
            "RIGHT_ELYTRA_PIVOT",
            "RightElytraPivot",
            "RightWingPivot",
            "RIGHT_WING_PIVOT",
        ],
    ),
    (
        ParentType::LeftElytra,
        &["LEFT_ELYTRA", "LeftElytron", "LEFT_ELYTRON", "LeftElytra"],
    ),
    (
        ParentType::RightElytra,
        &[
            "RIGHT_ELYTRA",
            "RightElytron",
            "RIGHT_ELYTRON",
            "RightElytra",
        ],
    ),
    (ParentType::Cape, &["Cape", "CAPE"]),
    (ParentType::World, &["World", "WORLD"]),
    (
        ParentType::Hud,
        &[
            "Hud",
            "HUD",
            "HeadsUpDisplay",
            "Gui",
            "GUI",
            "GraphicalUserInterface",
            "JraficalUserInterface",
        ],
    ),
    (
        ParentType::Camera,
        &["Camera", "CAMERA", "Billboard", "BILLBOARD"],
    ),
    (ParentType::Skull, &["Skull", "SKULL", "☠"]),
    (ParentType::Portrait, &["Portrait", "PORTRAIT"]),
    (ParentType::Arrow, &["Arrow", "ARROW"]),
    (ParentType::Trident, &["Trident", "TRIDENT"]),
    (ParentType::Item, &["Item", "ITEM"]),
    (
        ParentType::LeftItemPivot,
        &["LEFT_ITEM_PIVOT", "LeftItemPivot"],
    ),
    (
        ParentType::RightItemPivot,
        &["RIGHT_ITEM_PIVOT", "RightItemPivot"],
    ),
    (
        ParentType::LeftSpyglassPivot,
        &["LEFT_SPYGLASS_PIVOT", "LeftSpyglassPivot"],
    ),
    (
        ParentType::RightSpyglassPivot,
        &["RIGHT_SPYGLASS_PIVOT", "RightSpyglassPivot"],
    ),
    (
        ParentType::LeftParrotPivot,
        &["LEFT_PARROT_PIVOT", "LeftParrotPivot"],
    ),
    (
        ParentType::RightParrotPivot,
        &["RIGHT_PARROT_PIVOT", "RightParrotPivot"],
    ),
    (
        ParentType::HelmetItemPivot,
        &["HELMET_ITEM_PIVOT", "HelmetItemPivot"],
    ),
    (ParentType::HelmetPivot, &["HELMET_PIVOT", "HelmetPivot"]),
    (
        ParentType::ChestplatePivot,
        &[
            "CHESTPLATE_PIVOT",
            "ChestplatePivot",
            "ChestplateBodyPivot",
            "CHESTPLATE_BODY_PIVOT",
        ],
    ),
    (
        ParentType::LeftShoulderPivot,
        &["LEFT_SHOULDER_PIVOT", "LeftShoulderPivot"],
    ),
    (
        ParentType::RightShoulderPivot,
        &["RIGHT_SHOULDER_PIVOT", "RightShoulderPivot"],
    ),
    (
        ParentType::LeggingsPivot,
        &["LEGGINGS_PIVOT", "LeggingsPivot", "BeltPivot", "BELT_PIVOT"],
    ),
    (
        ParentType::LeftBootPivot,
        &["LEFT_BOOT_PIVOT", "LeftBootPivot"],
    ),
    (
        ParentType::RightBootPivot,
        &["RIGHT_BOOT_PIVOT", "RightBootPivot"],
    ),
];

#[cfg(test)]
mod tests {
    use super::ParentType;

    #[test]
    fn pivot_names_do_not_collapse_to_limb() {
        assert_eq!(
            ParentType::from_name("HelmetPivot"),
            ParentType::HelmetPivot
        );
        assert_eq!(
            ParentType::from_name("HelmetItemPivot"),
            ParentType::HelmetItemPivot
        );
        assert_eq!(
            ParentType::from_name("LeftLeggingPivot"),
            ParentType::LeftLeggingPivot
        );
        assert_eq!(ParentType::from_name("LeftArm"), ParentType::LeftArm);
        assert_eq!(ParentType::from_name("Head"), ParentType::Head);
    }

    #[test]
    fn lowercase_head_is_not_vanilla_head() {
        
        assert_eq!(ParentType::from_name("head"), ParentType::None);
        assert_eq!(ParentType::from_name("head1"), ParentType::None);
        assert_eq!(ParentType::from_name("HEAD"), ParentType::Head);
        assert_eq!(ParentType::from_name("Head"), ParentType::Head);
        
        assert_eq!(ParentType::from_name("RightArm2"), ParentType::RightArm);
    }
}
