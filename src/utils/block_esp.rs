use std::sync::{LazyLock, Mutex};

use jni::{
    errors::Result as JniResult,
    objects::{JObject, JValue},
};

use crate::{
    interface::Module,
    jni_bridge,
    utils::{
        geometry::{Aabb, Vec3d},
        minecraft::{self, MinecraftContext},
        settings,
    },
};

pub fn scan_blocks(config: &Module) {
    let range = settings::slider(config, RANGE_SETTING, 16.0) as i32;

    let positions = jni_bridge::with_env(|env| {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(Vec::new());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(Vec::new());
        }

        let player_x = ctx.env.call_method(&ctx.player, "getX", "()D", &[])?.d()?;
        let player_y = ctx.env.call_method(&ctx.player, "getY", "()D", &[])?.d()?;
        let player_z = ctx.env.call_method(&ctx.player, "getZ", "()D", &[])?.d()?;

        let show_ores = settings::checkbox(config, SHOW_ORES_SETTING, true);
        let show_chests = settings::checkbox(config, SHOW_CHESTS_SETTING, true);
        let show_misc = settings::checkbox(config, SHOW_MISC_SETTING, true);

        let targets = build_target_list(show_ores, show_chests, show_misc);

        scan_nearby_blocks(&mut ctx, player_x, player_y, player_z, range, &targets)
    })
    .unwrap_or_default();

    if let Ok(mut cache) = CACHED_POSITIONS.lock() {
        *cache = positions;
    }
}

fn build_target_list(ores: bool, chests: bool, misc: bool) -> Vec<TargetEntry> {
    let mut list = Vec::new();
    if ores {
        list.push(TargetEntry {
            key: "diamond_ore",
            color: [0.2, 0.8, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_diamond_ore",
            color: [0.2, 0.8, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "emerald_ore",
            color: [0.2, 1.0, 0.4, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_emerald_ore",
            color: [0.2, 1.0, 0.4, 1.0],
        });
        list.push(TargetEntry {
            key: "gold_ore",
            color: [1.0, 0.9, 0.1, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_gold_ore",
            color: [1.0, 0.9, 0.1, 1.0],
        });
        list.push(TargetEntry {
            key: "iron_ore",
            color: [0.9, 0.7, 0.5, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_iron_ore",
            color: [0.9, 0.7, 0.5, 1.0],
        });
        list.push(TargetEntry {
            key: "lapis_ore",
            color: [0.2, 0.4, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_lapis_ore",
            color: [0.2, 0.4, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "redstone_ore",
            color: [1.0, 0.2, 0.1, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_redstone_ore",
            color: [1.0, 0.2, 0.1, 1.0],
        });
        list.push(TargetEntry {
            key: "coal_ore",
            color: [0.3, 0.3, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_coal_ore",
            color: [0.3, 0.3, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "copper_ore",
            color: [1.0, 0.6, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "deepslate_copper_ore",
            color: [1.0, 0.6, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "nether_gold_ore",
            color: [1.0, 0.9, 0.1, 1.0],
        });
        list.push(TargetEntry {
            key: "nether_quartz_ore",
            color: [0.9, 0.9, 0.9, 1.0],
        });
        list.push(TargetEntry {
            key: "ancient_debris",
            color: [0.5, 0.2, 0.1, 1.0],
        });
    }
    if chests {
        list.push(TargetEntry {
            key: "chest",
            color: [1.0, 0.7, 0.0, 1.0],
        });
        list.push(TargetEntry {
            key: "trapped_chest",
            color: [1.0, 0.7, 0.0, 1.0],
        });
        list.push(TargetEntry {
            key: "ender_chest",
            color: [0.5, 0.0, 0.8, 1.0],
        });
        list.push(TargetEntry {
            key: "barrel",
            color: [1.0, 0.7, 0.0, 1.0],
        });
        list.push(TargetEntry {
            key: "shulker_box",
            color: [0.6, 0.2, 0.8, 1.0],
        });
    }
    if misc {
        list.push(TargetEntry {
            key: "spawner",
            color: [0.6, 0.0, 0.0, 1.0],
        });
        list.push(TargetEntry {
            key: "enchanting_table",
            color: [0.7, 0.3, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "anvil",
            color: [0.6, 0.6, 0.6, 1.0],
        });
        list.push(TargetEntry {
            key: "beacon",
            color: [0.4, 0.8, 1.0, 1.0],
        });
        list.push(TargetEntry {
            key: "end_portal_frame",
            color: [0.0, 0.5, 0.2, 1.0],
        });
        list.push(TargetEntry {
            key: "furnace",
            color: [0.4, 0.4, 0.4, 1.0],
        });
        list.push(TargetEntry {
            key: "blast_furnace",
            color: [0.4, 0.4, 0.4, 1.0],
        });
        list.push(TargetEntry {
            key: "smoker",
            color: [0.4, 0.4, 0.4, 1.0],
        });
        list.push(TargetEntry {
            key: "brewing_stand",
            color: [0.3, 0.7, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "hopper",
            color: [0.6, 0.4, 0.2, 1.0],
        });
        list.push(TargetEntry {
            key: "dispenser",
            color: [0.5, 0.5, 0.5, 1.0],
        });
        list.push(TargetEntry {
            key: "dropper",
            color: [0.5, 0.5, 0.5, 1.0],
        });
        list.push(TargetEntry {
            key: "crafting_table",
            color: [0.7, 0.5, 0.3, 1.0],
        });
        list.push(TargetEntry {
            key: "lectern",
            color: [0.6, 0.4, 0.2, 1.0],
        });
    }
    list
}

struct TargetEntry {
    key: &'static str,
    color: [f32; 4],
}

fn resolve_target<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    key: &str,
) -> Option<JObject<'local>> {
    let reg_class =
        match jni_bridge::load_class(ctx.env, "net.minecraft.core.registries.BuiltInRegistries") {
            Ok(c) => c,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                return None;
            }
        };

    let block_registry = match ctx.env.get_static_field(
        &reg_class,
        "BLOCK",
        "Lnet/minecraft/core/DefaultedRegistry;",
    ) {
        Ok(v) => v.l().ok(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(reg_class);
            return None;
        }
    }?;

    let id_text = if key.contains(':') {
        key.to_string()
    } else {
        format!("minecraft:{key}")
    };
    let path = match ctx.env.new_string(&id_text) {
        Ok(s) => JObject::from(s),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(block_registry);
            let _ = ctx.env.delete_local_ref(reg_class);
            return None;
        }
    };

    let loc = match ctx.env.call_static_method(
        "net/minecraft/resources/Identifier",
        "parse",
        "(Ljava/lang/String;)Lnet/minecraft/resources/Identifier;",
        &[JValue::Object(&path)],
    ) {
        Ok(v) => v.l().ok(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(path);
            let _ = ctx.env.delete_local_ref(block_registry);
            let _ = ctx.env.delete_local_ref(reg_class);
            return None;
        }
    }?;

    let result = match ctx.env.call_method(
        &block_registry,
        "getValue",
        "(Lnet/minecraft/resources/Identifier;)Ljava/lang/Object;",
        &[JValue::Object(&loc)],
    ) {
        Ok(v) => v.l().ok(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(loc);
            let _ = ctx.env.delete_local_ref(path);
            let _ = ctx.env.delete_local_ref(block_registry);
            let _ = ctx.env.delete_local_ref(reg_class);
            return None;
        }
    };

    let _ = ctx.env.delete_local_ref(loc);
    let _ = ctx.env.delete_local_ref(path);
    let _ = ctx.env.delete_local_ref(block_registry);
    let _ = ctx.env.delete_local_ref(reg_class);

    result
}

fn scan_nearby_blocks(
    ctx: &mut MinecraftContext<'_, '_>,
    px: f64,
    py: f64,
    pz: f64,
    range: i32,
    targets: &[TargetEntry],
) -> JniResult<Vec<BlockWorldPos>> {
    let bx = px.floor() as i32;
    let by = py.floor() as i32;
    let bz = pz.floor() as i32;

    let mut resolved: Vec<Option<(JObject<'_>, [f32; 4])>> = Vec::new();
    for t in targets {
        if let Some(block) = resolve_target(ctx, t.key) {
            resolved.push(Some((block, t.color)));
        }
    }

    let mut results = Vec::new();
    for dx in -range..=range {
        for dz in -range..=range {
            for dy in (-4..=4).step_by(1) {
                let x = bx + dx;
                let y = by + dy;
                let z = bz + dz;

                let result = check_block_pos(ctx, x, y, z, &resolved);
                match result {
                    Ok(Some(wpos)) => results.push(wpos),
                    Ok(None) => {}
                    Err(_) => {
                        jni_bridge::clear_pending_exception(ctx.env);
                    }
                }
            }
        }
    }

    for entry in &mut resolved {
        if let Some((block, _)) = entry.take() {
            let _ = ctx.env.delete_local_ref(block);
        }
    }

    Ok(results)
}

fn check_block_pos(
    ctx: &mut MinecraftContext<'_, '_>,
    x: i32,
    y: i32,
    z: i32,
    targets: &[Option<(JObject<'_>, [f32; 4])>],
) -> JniResult<Option<BlockWorldPos>> {
    let pos = ctx.env.new_object(
        "net/minecraft/core/BlockPos",
        "(III)V",
        &[JValue::Int(x), JValue::Int(y), JValue::Int(z)],
    )?;
    let state = ctx
        .env
        .call_method(
            &ctx.level,
            "getBlockState",
            "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
            &[JValue::Object(&pos)],
        )?
        .l()?;

    let is_air = ctx.env.call_method(&state, "isAir", "()Z", &[])?.z()?;
    if is_air {
        let _ = ctx.env.delete_local_ref(state);
        let _ = ctx.env.delete_local_ref(pos);
        return Ok(None);
    }

    let block = ctx
        .env
        .call_method(
            &state,
            "getBlock",
            "()Lnet/minecraft/world/level/block/Block;",
            &[],
        )?
        .l()?;
    let _ = ctx.env.delete_local_ref(state);
    let _ = ctx.env.delete_local_ref(pos);

    let mut matched_color = None;
    for entry in targets {
        if let Some((target_block, color)) = entry {
            if ctx.env.is_same_object(&block, target_block)? {
                matched_color = Some(*color);
                break;
            }
        }
    }

    let _ = ctx.env.delete_local_ref(block);
    Ok(matched_color.map(|c| BlockWorldPos {
        x: x as f64,
        y: y as f64,
        z: z as f64,
        color: c,
    }))
}

pub const RANGE_SETTING: &str = "Range";
pub const BOX_COLOR_SETTING: &str = "Box color";
pub const SHOW_ORES_SETTING: &str = "Show ores";
pub const SHOW_CHESTS_SETTING: &str = "Show chests";
pub const SHOW_MISC_SETTING: &str = "Show misc";
pub const BOX_MODE_SETTING: &str = "Box mode";
pub const FILLED_MODE_SETTING: &str = "Filled mode";

#[derive(Clone, Copy, Debug)]
struct BlockWorldPos {
    x: f64,
    y: f64,
    z: f64,
    color: [f32; 4],
}

impl BlockWorldPos {
    fn aabb(self) -> Aabb {
        Aabb {
            min_x: self.x,
            min_y: self.y,
            min_z: self.z,
            max_x: self.x + 1.0,
            max_y: self.y + 1.0,
            max_z: self.z + 1.0,
        }
    }
}

static CACHED_POSITIONS: LazyLock<Mutex<Vec<BlockWorldPos>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));


fn cached_positions() -> Vec<BlockWorldPos> {
    CACHED_POSITIONS
        .lock()
        .ok()
        .map(|c| c.clone())
        .unwrap_or_default()
}







pub fn build_world_geometry(
    lines: &mut crate::avatar::render::mesh::Mesh,
    _filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) {
    for position in cached_positions() {
        let aabb = position.aabb();
        crate::world_render::geometry::push_box_outline(
            lines,
            camera,
            crate::world_render::geometry::Box3 {
                min: Vec3d {
                    x: aabb.min_x,
                    y: aabb.min_y,
                    z: aabb.min_z,
                },
                max: Vec3d {
                    x: aabb.max_x,
                    y: aabb.max_y,
                    z: aabb.max_z,
                },
            },
            position.color,
        );
    }
}
