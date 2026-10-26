use std::collections::HashSet;

use jvmti_bindings::env::Jvmti;
use ristretto_classfile::{
    ClassFile, ConstantPool,
    attributes::{Attribute, Instruction},
};

use crate::jni_bridge;

pub const GAME_VERSION: &str = "26.2";
pub const PROTOCOL_VERSION: i32 = 776;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLoader {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl RuntimeLoader {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MethodSymbol {
    pub name: &'static str,
    pub descriptor: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderSymbols {
    pub living_entity_renderer: &'static str,
    pub living_submit: MethodSymbol,
    pub avatar_renderer: &'static str,
    pub render_right_hand: MethodSymbol,
    pub render_left_hand: MethodSymbol,
    pub entity_render_dispatcher: &'static str,
    pub dispatcher_submit: MethodSymbol,
    pub level_renderer: &'static str,
    pub submit_entities: MethodSymbol,
    pub submit_node_collector: &'static str,
    pub custom_geometry_renderer: &'static str,
    pub custom_geometry_render: MethodSymbol,
    pub submit_custom_geometry: MethodSymbol,
    pub pose_stack: &'static str,
    pub pose: &'static str,
    pub vertex_consumer: &'static str,
    pub render_type: &'static str,
}

pub const RENDER_SYMBOLS: RenderSymbols = RenderSymbols {
    living_entity_renderer: "net/minecraft/client/renderer/entity/LivingEntityRenderer",
    living_submit: MethodSymbol {
        name: "submit",
        descriptor: "(Lnet/minecraft/client/renderer/entity/state/LivingEntityRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;Lnet/minecraft/client/renderer/state/level/CameraRenderState;)V",
    },
    avatar_renderer: "net/minecraft/client/renderer/entity/player/AvatarRenderer",
    render_right_hand: MethodSymbol {
        name: "renderRightHand",
        descriptor: "(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;ILnet/minecraft/resources/Identifier;Z)V",
    },
    render_left_hand: MethodSymbol {
        name: "renderLeftHand",
        descriptor: "(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;ILnet/minecraft/resources/Identifier;Z)V",
    },
    entity_render_dispatcher: "net/minecraft/client/renderer/entity/EntityRenderDispatcher",
    dispatcher_submit: MethodSymbol {
        name: "submit",
        descriptor: "(Lnet/minecraft/client/renderer/entity/state/EntityRenderState;Lnet/minecraft/client/renderer/state/level/CameraRenderState;DDDLcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;)V",
    },
    level_renderer: "net/minecraft/client/renderer/LevelRenderer",
    submit_entities: MethodSymbol {
        name: "submitEntities",
        descriptor: "(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/state/level/LevelRenderState;Lnet/minecraft/client/renderer/SubmitNodeCollector;)V",
    },
    submit_node_collector: "net/minecraft/client/renderer/SubmitNodeCollector",
    custom_geometry_renderer: "net/minecraft/client/renderer/SubmitNodeCollector$CustomGeometryRenderer",
    custom_geometry_render: MethodSymbol {
        name: "render",
        descriptor: "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;)V",
    },
    submit_custom_geometry: MethodSymbol {
        name: "submitCustomGeometry",
        descriptor: "(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/rendertype/RenderType;Lnet/minecraft/client/renderer/SubmitNodeCollector$CustomGeometryRenderer;)V",
    },
    pose_stack: "com/mojang/blaze3d/vertex/PoseStack",
    pose: "com/mojang/blaze3d/vertex/PoseStack$Pose",
    vertex_consumer: "com/mojang/blaze3d/vertex/VertexConsumer",
    render_type: "net/minecraft/client/renderer/rendertype/RenderType",
};

#[derive(Clone, Debug)]
pub struct RuntimeProfile {
    pub loader: RuntimeLoader,
    pub game_version: String,
    pub protocol_version: i32,
    pub render: &'static RenderSymbols,
}

#[derive(Clone, Debug, Default)]
pub struct ProfileValidation {
    pub present_classes: usize,
    pub missing_classes: Vec<&'static str>,
}

impl RuntimeProfile {
    pub fn detect(jvmti: &Jvmti) -> Self {
        let signatures = loaded_signatures(jvmti);
        
        
        let loader = if signatures
            .iter()
            .any(|signature| signature.starts_with("Lnet/fabricmc/"))
            || signatures
                .iter()
                .any(|signature| signature.starts_with("Lorg/quiltmc/"))
        {
            RuntimeLoader::Fabric
        } else if signatures.iter().any(|signature| {
            signature.starts_with("Lnet/neoforged/fml/")
                || signature.starts_with("Lnet/neoforged/neoforge/")
        }) {
            RuntimeLoader::NeoForge
        } else if signatures
            .iter()
            .any(|signature| signature.starts_with("Lnet/minecraftforge/"))
        {
            RuntimeLoader::Forge
        } else {
            RuntimeLoader::Vanilla
        };

        let (game_version, protocol_version) =
            query_game_version().unwrap_or_else(|| (GAME_VERSION.to_string(), PROTOCOL_VERSION));
        Self {
            loader,
            game_version,
            protocol_version,
            render: &RENDER_SYMBOLS,
        }
    }

    pub fn id(&self) -> String {
        format!("{}-{}", self.loader.id(), self.game_version)
    }

    pub fn is_supported(&self) -> bool {
        self.game_version == GAME_VERSION && self.protocol_version == PROTOCOL_VERSION
    }

    pub fn validate_loaded_classes(&self, jvmti: &Jvmti) -> ProfileValidation {
        let signatures = loaded_signatures(jvmti);
        let names = signatures
            .iter()
            .filter_map(|signature| {
                signature
                    .strip_prefix('L')
                    .and_then(|value| value.strip_suffix(';'))
            })
            .collect::<HashSet<_>>();
        let required = [
            self.render.living_entity_renderer,
            self.render.avatar_renderer,
            self.render.entity_render_dispatcher,
            self.render.level_renderer,
            self.render.submit_node_collector,
            self.render.custom_geometry_renderer,
            self.render.pose_stack,
            self.render.vertex_consumer,
            self.render.render_type,
        ];
        let mut result = ProfileValidation::default();
        for class in required {
            if names.contains(class) {
                result.present_classes += 1;
            } else {
                result.missing_classes.push(class);
            }
        }
        result
    }
}

pub fn validate_method(class_bytes: &[u8], method: MethodSymbol) -> Result<(), String> {
    let class_file = ClassFile::from_bytes(class_bytes).map_err(|error| error.to_string())?;
    find_method(&class_file, method)
        .map(|_| ())
        .ok_or_else(|| format!("missing {}{}", method.name, method.descriptor))
}

pub fn validate_target_class(class_name: &str, class_bytes: &[u8]) -> Result<(), String> {
    let class_file = ClassFile::from_bytes(class_bytes).map_err(|error| error.to_string())?;
    match class_name {
        name if name == RENDER_SYMBOLS.living_entity_renderer => {
            find_method(&class_file, RENDER_SYMBOLS.living_submit)
                .map(|_| ())
                .ok_or_else(|| {
                    format!(
                        "missing {}{}",
                        RENDER_SYMBOLS.living_submit.name, RENDER_SYMBOLS.living_submit.descriptor
                    )
                })
        }
        name if name == RENDER_SYMBOLS.avatar_renderer => {
            validate_fingerprint(
                &class_file,
                RENDER_SYMBOLS.render_right_hand,
                5,
                &["renderHand"],
            )?;
            validate_fingerprint(
                &class_file,
                RENDER_SYMBOLS.render_left_hand,
                5,
                &["renderHand"],
            )
        }
        name if name == RENDER_SYMBOLS.entity_render_dispatcher => {
            
            find_method(&class_file, RENDER_SYMBOLS.dispatcher_submit)
                .map(|_| ())
                .ok_or_else(|| {
                    format!(
                        "missing {}{}",
                        RENDER_SYMBOLS.dispatcher_submit.name,
                        RENDER_SYMBOLS.dispatcher_submit.descriptor
                    )
                })
        }
        name if name == RENDER_SYMBOLS.level_renderer => {
            validate_fingerprint(&class_file, RENDER_SYMBOLS.submit_entities, 12, &["submit"])
        }
        name if name == RENDER_SYMBOLS.custom_geometry_renderer => {
            find_method(&class_file, RENDER_SYMBOLS.custom_geometry_render)
                .map(|_| ())
                .ok_or_else(|| {
                    format!(
                        "missing {}{}",
                        RENDER_SYMBOLS.custom_geometry_render.name,
                        RENDER_SYMBOLS.custom_geometry_render.descriptor
                    )
                })
        }
        _ => Ok(()),
    }
}

fn find_method<'a>(
    class_file: &'a ClassFile<'_>,
    method: MethodSymbol,
) -> Option<&'a ristretto_classfile::Method> {
    class_file.methods.iter().find(|candidate| {
        let Ok(name) = class_file.constant_pool.try_get_utf8(candidate.name_index) else {
            return false;
        };
        let Ok(descriptor) = class_file
            .constant_pool
            .try_get_utf8(candidate.descriptor_index)
        else {
            return false;
        };
        name.to_str_lossy() == method.name && descriptor.to_str_lossy() == method.descriptor
    })
}

fn validate_fingerprint(
    class_file: &ClassFile<'_>,
    method: MethodSymbol,
    minimum_instructions: usize,
    required_calls: &[&str],
) -> Result<(), String> {
    let method_info = find_method(class_file, method)
        .ok_or_else(|| format!("missing {}{}", method.name, method.descriptor))?;
    let code = method_info
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            Attribute::Code { code, .. } => Some(code),
            _ => None,
        })
        .ok_or_else(|| format!("{}{} has no Code attribute", method.name, method.descriptor))?;
    if code.len() < minimum_instructions {
        return Err(format!(
            "{}{} bytecode is too short ({} < {})",
            method.name,
            method.descriptor,
            code.len(),
            minimum_instructions
        ));
    }

    let calls = code
        .iter()
        .filter_map(|instruction| invocation_name(&class_file.constant_pool, instruction))
        .collect::<HashSet<_>>();
    for required in required_calls {
        if !calls.contains(*required) {
            return Err(format!(
                "{}{} fingerprint lacks call {required}",
                method.name, method.descriptor
            ));
        }
    }
    Ok(())
}

fn invocation_name(constant_pool: &ConstantPool<'_>, instruction: &Instruction) -> Option<String> {
    let name_and_type = match instruction {
        Instruction::Invokevirtual(index)
        | Instruction::Invokespecial(index)
        | Instruction::Invokestatic(index) => *constant_pool.try_get_method_ref(*index).ok()?.1,
        Instruction::Invokeinterface(index, _) => {
            *constant_pool.try_get_interface_method_ref(*index).ok()?.1
        }
        _ => return None,
    };
    let (name_index, _) = constant_pool.try_get_name_and_type(name_and_type).ok()?;
    Some(
        constant_pool
            .try_get_utf8(*name_index)
            .ok()?
            .to_str_lossy()
            .into_owned(),
    )
}

fn loaded_signatures(jvmti: &Jvmti) -> Vec<String> {
    let Ok(classes) = jvmti.get_loaded_classes() else {
        return Vec::new();
    };
    classes
        .into_iter()
        .filter_map(|class| jvmti.get_class_signature(class).ok().map(|item| item.0))
        .collect()
}

fn query_game_version() -> Option<(String, i32)> {
    jni_bridge::with_env(|env| {
        let version = env
            .call_static_method(
                "net/minecraft/SharedConstants",
                "getCurrentVersion",
                "()Lnet/minecraft/WorldVersion;",
                &[],
            )?
            .l()?;
        let id = env
            .call_method(&version, "id", "()Ljava/lang/String;", &[])?
            .l()?;
        let protocol = env
            .call_method(&version, "protocolVersion", "()I", &[])?
            .i()?;
        let id = env.get_string((&id).into())?.into();
        Ok((id, protocol))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, io::Read};

    #[test]
    fn all_26_2_loaders_share_unobfuscated_render_symbols() {
        for loader in [
            RuntimeLoader::Vanilla,
            RuntimeLoader::Fabric,
            RuntimeLoader::Forge,
            RuntimeLoader::NeoForge,
        ] {
            let profile = RuntimeProfile {
                loader,
                game_version: GAME_VERSION.to_string(),
                protocol_version: PROTOCOL_VERSION,
                render: &RENDER_SYMBOLS,
            };
            assert!(profile.is_supported());
            assert_eq!(
                profile.render.avatar_renderer,
                "net/minecraft/client/renderer/entity/player/AvatarRenderer"
            );
        }
    }

    
    
    
    #[test]
    fn level_renderer_passes_its_fingerprint_gate() {
        let Ok(path) = std::env::var("XZ_MC_26_2_CLIENT_JAR") else {
            return;
        };
        let file = File::open(path).expect("open client fixture");
        let mut archive = zip::ZipArchive::new(file).expect("open jar");
        let mut entry = archive
            .by_name(&format!("{}.class", RENDER_SYMBOLS.level_renderer))
            .expect("LevelRenderer in jar");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read class");
        validate_target_class(RENDER_SYMBOLS.level_renderer, &bytes)
            .expect("LevelRenderer must pass validation or the world hook is dead");
    }

    #[test]
    fn rejects_wrong_protocol_or_version() {
        let wrong = RuntimeProfile {
            loader: RuntimeLoader::Vanilla,
            game_version: "26.1.2".to_string(),
            protocol_version: PROTOCOL_VERSION,
            render: &RENDER_SYMBOLS,
        };
        assert!(!wrong.is_supported());
    }

    #[test]
    fn validates_official_client_when_fixture_is_available() {
        let Ok(path) = std::env::var("XZ_MC_26_2_CLIENT_JAR") else {
            return;
        };
        let file = File::open(path).expect("open 26.2 client fixture");
        let mut archive = zip::ZipArchive::new(file).expect("open client jar");
        for class_name in [
            RENDER_SYMBOLS.living_entity_renderer,
            RENDER_SYMBOLS.avatar_renderer,
            RENDER_SYMBOLS.entity_render_dispatcher,
            RENDER_SYMBOLS.level_renderer,
            RENDER_SYMBOLS.custom_geometry_renderer,
        ] {
            let mut entry = archive
                .by_name(&format!("{class_name}.class"))
                .expect("class in client jar");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read class bytes");
            validate_target_class(class_name, &bytes)
                .unwrap_or_else(|error| panic!("{class_name}: {error}"));
        }
    }
}
