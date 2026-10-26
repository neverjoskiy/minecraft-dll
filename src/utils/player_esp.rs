use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
};

use crate::{
    interface::Module,
    jni_bridge,
    utils::{
        friends,
        geometry::{Aabb, Vec3d},
        minecraft::{self, MinecraftContext},
        name_tags::{self, NameTagItemIconRequest, NameTagItemSlot},
        settings,
    },
};

pub const RANGE_SETTING: &str = "Range";
pub const BOX_SETTING: &str = "Box";
pub const NAME_SETTING: &str = "Name";
pub const HEALTH_SETTING: &str = "Health";
pub const HAND_ITEM_SETTING: &str = "Hand item";
pub const SHOW_FRIENDS_SETTING: &str = "Show friends";
pub const SHOW_SELF_SETTING: &str = "Show self";
pub const BOX_COLOR_SETTING: &str = "Box color";
pub const FRIEND_COLOR_SETTING: &str = "Friend color";
pub const CHAMS_SETTING: &str = "Chams";

const SCREEN_MARGIN: f32 = 64.0;

#[derive(Clone, Debug)]
pub struct PlayerEspConfig {
    pub range: f32,
    pub show_box: bool,
    pub show_chams: bool,
    pub show_name: bool,
    pub show_health: bool,
    pub show_hand_item: bool,
    pub show_friends: bool,
    pub show_self: bool,
    pub box_color: [f32; 4],
    pub friend_color: [f32; 4],
}

impl PlayerEspConfig {
    pub fn from_module(module: &Module) -> Self {
        Self {
            range: settings::slider(module, RANGE_SETTING, 64.0),
            show_box: settings::checkbox(module, BOX_SETTING, true),
            show_chams: settings::checkbox(module, CHAMS_SETTING, false),
            show_name: settings::checkbox(module, NAME_SETTING, true),
            show_health: settings::checkbox(module, HEALTH_SETTING, true),
            show_hand_item: settings::checkbox(module, HAND_ITEM_SETTING, true),
            show_friends: settings::checkbox(module, SHOW_FRIENDS_SETTING, true),
            show_self: settings::checkbox(module, SHOW_SELF_SETTING, false),
            box_color: settings::color_picker(module, BOX_COLOR_SETTING, [1.0, 1.0, 1.0, 1.0]),
            friend_color: settings::color_picker(
                module,
                FRIEND_COLOR_SETTING,
                [0.25, 0.85, 0.45, 1.0],
            ),
        }
    }

    pub fn needs_overlay(&self) -> bool {
        self.show_box || self.show_name || self.show_health || self.show_hand_item
    }

    pub fn needs_chams(&self) -> bool {
        self.show_chams
    }
}

#[derive(Clone, Debug)]
pub struct PlayerEspEntry {
    pub entity_id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub health: f32,
    pub health_ratio: f32,
    pub health_color: [u8; 3],
    pub name_color: [u8; 3],
    pub box_color: [f32; 4],
    pub has_main_hand: bool,
}

pub fn hand_item_icon_requests(entries: &[PlayerEspEntry]) -> Vec<NameTagItemIconRequest> {
    let mut requests = Vec::new();
    for entry in entries {
        if !entry.has_main_hand {
            continue;
        }
        let size = (entry.width * 0.42).clamp(12.0, 22.0);
        requests.push(NameTagItemIconRequest {
            entity_id: entry.entity_id,
            slot: NameTagItemSlot::MainHand,
            inventory_slot: None,
            x: entry.x + entry.width * 0.5 - size * 0.5,
            y: entry.y + entry.height + 3.0,
            size,
        });
    }
    requests
}

fn collect_player(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    config: &PlayerEspConfig,
    viewport: [f32; 2],
    camera: &name_tags::CameraView,
    player_pos: Vec3d,
    player_team: &Option<String>,
    partial_ticks: f32,
) -> JniResult<Option<PlayerEspEntry>> {
    let is_self = ctx.env.is_same_object(entity, &ctx.player)?;
    if is_self && !config.show_self {
        return Ok(None);
    }

    if !minecraft::entity_is_alive(ctx, entity)? {
        return Ok(None);
    }

    let entity_pos = entity_position(ctx.env, entity, partial_ticks)?;
    if player_pos.distance_to(entity_pos) as f32 > config.range {
        return Ok(None);
    }

    let entity_team = match team_name(ctx.env, entity) {
        Ok(team) => team,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            None
        }
    };
    let same_team = player_team.is_some() && player_team == &entity_team;
    let scoreboard_name = match minecraft::entity_scoreboard_name(ctx, entity) {
        Ok(name) => name,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            String::new()
        }
    };
    let explicit_friend = friends::is_friend(&scoreboard_name);
    if (same_team || explicit_friend) && !config.show_friends {
        return Ok(None);
    }

    let Some((x, y, width, height, depth)) =
        project_entity_box(ctx, entity, entity_pos, camera, viewport, partial_ticks)?
    else {
        return Ok(None);
    };

    if x + width < -SCREEN_MARGIN
        || y + height < -SCREEN_MARGIN
        || x > viewport[0] + SCREEN_MARGIN
        || y > viewport[1] + SCREEN_MARGIN
    {
        return Ok(None);
    }

    let health = optional_float(ctx.env, entity, "getHealth", 20.0)
        + optional_float(ctx.env, entity, "getAbsorptionAmount", 0.0);
    let max_health = optional_float(ctx.env, entity, "getMaxHealth", 20.0).max(1.0)
        + optional_float(ctx.env, entity, "getAbsorptionAmount", 0.0);
    let health_ratio = (health / max_health.max(1.0)).clamp(0.0, 1.0);

    let name = match entity_display_name(ctx.env, entity) {
        Ok(name) if !name.is_empty() => name,
        _ => {
            jni_bridge::clear_pending_exception(ctx.env);
            if scoreboard_name.is_empty() {
                "Player".to_string()
            } else {
                scoreboard_name
            }
        }
    };

    let name_color = if explicit_friend || same_team {
        color_picker_rgb(config.friend_color)
    } else {
        [245, 247, 250]
    };
    let box_color = if explicit_friend || same_team {
        config.friend_color
    } else {
        config.box_color
    };

    let entity_id = ctx.env.call_method(entity, "getId", "()I", &[])?.i()?;
    let has_main_hand = config.show_hand_item && has_main_hand_item(ctx.env, entity)?;

    Ok(Some(PlayerEspEntry {
        entity_id,
        name,
        x,
        y,
        width,
        height,
        depth,
        health,
        health_ratio,
        health_color: health_color(health_ratio),
        name_color,
        box_color,
        has_main_hand,
    }))
}

fn project_entity_box(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    entity_pos: Vec3d,
    camera: &name_tags::CameraView,
    viewport: [f32; 2],
    partial_ticks: f32,
) -> JniResult<Option<(f32, f32, f32, f32, f32)>> {
    let aabb = render_aabb(ctx, entity, entity_pos, partial_ticks)?;
    let corners = aabb_corners(aabb);

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut depth = f64::MAX;
    let mut projected = 0usize;

    for corner in &corners {
        let relative = Vec3d {
            x: corner.x - camera.position.x,
            y: corner.y - camera.position.y,
            z: corner.z - camera.position.z,
        };
        let corner_depth = relative.dot(camera.forward);
        if corner_depth < depth {
            depth = corner_depth;
        }
        if corner_depth <= 0.05 {
            continue;
        }
        if let Some(screen) = name_tags::manual_project_to_screen(*corner, camera, viewport) {
            projected += 1;
            min_x = min_x.min(screen.x);
            min_y = min_y.min(screen.y);
            max_x = max_x.max(screen.x);
            max_y = max_y.max(screen.y);
        }
    }

    if projected < 2
        || !(min_x.is_finite() && min_y.is_finite())
        || max_x <= min_x
        || max_y <= min_y
    {
        return Ok(None);
    }

    
    let width = (max_x - min_x).max(4.0);
    let height = (max_y - min_y).max(8.0);
    Ok(Some((min_x, min_y, width, height, depth as f32)))
}

fn render_aabb(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    entity_pos: Vec3d,
    partial_ticks: f32,
) -> JniResult<Aabb> {
    match minecraft::entity_aabb(ctx, entity) {
        Ok(aabb) => {
            let current = Vec3d {
                x: ctx.env.call_method(entity, "getX", "()D", &[])?.d()?,
                y: ctx.env.call_method(entity, "getY", "()D", &[])?.d()?,
                z: ctx.env.call_method(entity, "getZ", "()D", &[])?.d()?,
            };
            let dx = entity_pos.x - current.x;
            let dy = entity_pos.y - current.y;
            let dz = entity_pos.z - current.z;
            Ok(Aabb {
                min_x: aabb.min_x + dx,
                min_y: aabb.min_y + dy,
                min_z: aabb.min_z + dz,
                max_x: aabb.max_x + dx,
                max_y: aabb.max_y + dy,
                max_z: aabb.max_z + dz,
            })
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let width = optional_float(ctx.env, entity, "getBbWidth", 0.6) as f64;
            let height = optional_float(ctx.env, entity, "getBbHeight", 1.8) as f64;
            let half = width * 0.5;
            let _ = partial_ticks;
            Ok(Aabb {
                min_x: entity_pos.x - half,
                min_y: entity_pos.y,
                min_z: entity_pos.z - half,
                max_x: entity_pos.x + half,
                max_y: entity_pos.y + height,
                max_z: entity_pos.z + half,
            })
        }
    }
}

fn aabb_corners(aabb: Aabb) -> [Vec3d; 8] {
    [
        Vec3d {
            x: aabb.min_x,
            y: aabb.min_y,
            z: aabb.min_z,
        },
        Vec3d {
            x: aabb.max_x,
            y: aabb.min_y,
            z: aabb.min_z,
        },
        Vec3d {
            x: aabb.min_x,
            y: aabb.max_y,
            z: aabb.min_z,
        },
        Vec3d {
            x: aabb.max_x,
            y: aabb.max_y,
            z: aabb.min_z,
        },
        Vec3d {
            x: aabb.min_x,
            y: aabb.min_y,
            z: aabb.max_z,
        },
        Vec3d {
            x: aabb.max_x,
            y: aabb.min_y,
            z: aabb.max_z,
        },
        Vec3d {
            x: aabb.min_x,
            y: aabb.max_y,
            z: aabb.max_z,
        },
        Vec3d {
            x: aabb.max_x,
            y: aabb.max_y,
            z: aabb.max_z,
        },
    ]
}

fn player_iterable<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    match ctx
        .env
        .call_method(&ctx.level, "players", "()Ljava/util/List;", &[])
    {
        Ok(value) => value.l(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env
                .call_method(
                    &ctx.level,
                    "entitiesForRendering",
                    "()Ljava/lang/Iterable;",
                    &[],
                )?
                .l()
        }
    }
}

fn has_main_hand_item(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<bool> {
    let stack = match env.call_method(
        entity,
        "getMainHandItem",
        "()Lnet/minecraft/world/item/ItemStack;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(false);
        }
    };
    if stack.is_null() {
        return Ok(false);
    }
    let empty = env.call_method(&stack, "isEmpty", "()Z", &[])?.z()?;
    let _ = env.delete_local_ref(stack);
    Ok(!empty)
}

fn entity_display_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<String> {
    let component = env
        .call_method(
            entity,
            "getName",
            "()Lnet/minecraft/network/chat/Component;",
            &[],
        )?
        .l()?;
    if component.is_null() {
        return Ok(String::new());
    }
    let string = env
        .call_method(&component, "getString", "()Ljava/lang/String;", &[])?
        .l()?;
    let text = if string.is_null() {
        String::new()
    } else {
        let result = env
            .get_string((&string).into())?
            .to_string_lossy()
            .into_owned();
        let _ = env.delete_local_ref(string);
        result
    };
    let _ = env.delete_local_ref(component);
    Ok(strip_format_codes(&text))
}

fn team_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<Option<String>> {
    let team = match env.call_method(
        entity,
        "getTeam",
        "()Lnet/minecraft/world/scores/Team;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    if team.is_null() {
        return Ok(None);
    }
    let name = match env.call_method(&team, "getName", "()Ljava/lang/String;", &[]) {
        Ok(value) => {
            let string = value.l()?;
            if string.is_null() {
                None
            } else {
                let text = env
                    .get_string((&string).into())?
                    .to_string_lossy()
                    .into_owned();
                let _ = env.delete_local_ref(string);
                Some(text)
            }
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            None
        }
    };
    let _ = env.delete_local_ref(team);
    Ok(name)
}

fn entity_position(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    partial_ticks: f32,
) -> JniResult<Vec3d> {
    match env.call_method(
        entity,
        "getPosition",
        "(F)Lnet/minecraft/world/phys/Vec3;",
        &[JValue::Float(partial_ticks)],
    ) {
        Ok(value) => {
            let position = value.l()?;
            let result = Vec3d {
                x: env.get_field(&position, "x", "D")?.d()?,
                y: env.get_field(&position, "y", "D")?.d()?,
                z: env.get_field(&position, "z", "D")?.d()?,
            };
            let _ = env.delete_local_ref(position);
            Ok(result)
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            Ok(Vec3d {
                x: env.call_method(entity, "getX", "()D", &[])?.d()?,
                y: env.call_method(entity, "getY", "()D", &[])?.d()?,
                z: env.call_method(entity, "getZ", "()D", &[])?.d()?,
            })
        }
    }
}

fn frame_partial_ticks(ctx: &mut MinecraftContext<'_, '_>) -> f32 {
    let partial_ticks = (|| -> JniResult<f32> {
        let delta_tracker = ctx
            .env
            .call_method(
                &ctx.minecraft,
                "getDeltaTracker",
                "()Lnet/minecraft/client/DeltaTracker;",
                &[],
            )?
            .l()?;
        let value = ctx
            .env
            .call_method(
                &delta_tracker,
                "getGameTimeDeltaPartialTick",
                "(Z)F",
                &[JValue::Bool(0)],
            )?
            .f()?;
        let _ = ctx.env.delete_local_ref(delta_tracker);
        Ok(value)
    })();

    match partial_ticks {
        Ok(value) if value.is_finite() => value.clamp(0.0, 1.0),
        _ => {
            jni_bridge::clear_pending_exception(ctx.env);
            1.0
        }
    }
}

fn optional_float(env: &mut JNIEnv<'_>, object: &JObject<'_>, method: &str, default: f32) -> f32 {
    match env.call_method(object, method, "()F", &[]) {
        Ok(value) => value.f().unwrap_or(default),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            default
        }
    }
}

fn health_color(ratio: f32) -> [u8; 3] {
    if ratio > 0.75 {
        [0, 255, 0]
    } else if ratio > 0.5 {
        [255, 255, 0]
    } else if ratio > 0.25 {
        [255, 170, 0]
    } else {
        [255, 85, 85]
    }
}

fn color_picker_rgb(color: [f32; 4]) -> [u8; 3] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn strip_format_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if (ch == '&' || ch as u32 == 0x00A7)
            && chars.peek().is_some_and(|next| is_format_code(*next))
        {
            let _ = chars.next();
            continue;
        }
        result.push(ch);
    }
    result
}

fn is_format_code(ch: char) -> bool {
    matches!(
        ch,
        '0'..='9'
            | 'a'..='f'
            | 'A'..='F'
            | 'k'..='o'
            | 'K'..='O'
            | 'r'
            | 'R'
            | 'x'
            | 'X'
    )
}



static WORLD_CONFIG: std::sync::Mutex<Option<PlayerEspConfig>> = std::sync::Mutex::new(None);

pub fn set_world_config(config: Option<PlayerEspConfig>) {
    if let Ok(mut slot) = WORLD_CONFIG.lock() {
        *slot = config;
    }
}




pub fn build_world_geometry(
    env: &mut JNIEnv<'_>,
    lines: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) -> JniResult<()> {
    let Some(config) = WORLD_CONFIG.lock().ok().and_then(|slot| slot.clone()) else {
        return Ok(());
    };
    if !config.show_box {
        return Ok(());
    }
    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(());
    }

    let player_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let player_team = team_name(ctx.env, &ctx.player).unwrap_or(None);
    let partial_ticks = frame_partial_ticks(&mut ctx);
    let player_pos = entity_position(ctx.env, &ctx.player, partial_ticks)?;

    let iterable = player_iterable(&mut ctx)?;
    let iterator = ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        if ctx.env.is_instance_of(&entity, &player_class).unwrap_or(false)
            && let Err(_) = push_entity_box(
                &mut ctx,
                &entity,
                &config,
                player_pos,
                &player_team,
                partial_ticks,
                lines,
                camera,
            )
        {
            jni_bridge::clear_pending_exception(ctx.env);
        }
        let _ = ctx.env.delete_local_ref(entity);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_entity_box(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    config: &PlayerEspConfig,
    player_pos: Vec3d,
    player_team: &Option<String>,
    partial_ticks: f32,
    lines: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) -> JniResult<()> {
    let is_self = ctx.env.is_same_object(entity, &ctx.player)?;
    if is_self && !config.show_self {
        return Ok(());
    }
    if !minecraft::entity_is_alive(ctx, entity)? {
        return Ok(());
    }
    
    if let Some(uuid) = entity_uuid(ctx, entity)
        && crate::world_render::skip_player(uuid)
    {
        return Ok(());
    }
    let entity_pos = entity_position(ctx.env, entity, partial_ticks)?;
    if player_pos.distance_to(entity_pos) as f32 > config.range {
        return Ok(());
    }
    let entity_team = team_name(ctx.env, entity).unwrap_or(None);
    let is_friend = player_team.is_some() && player_team == &entity_team;
    if is_friend && !config.show_friends {
        return Ok(());
    }
    let color = if is_friend {
        config.friend_color
    } else {
        config.box_color
    };

    let aabb = minecraft::entity_aabb(ctx, entity)?;
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
        color,
    );
    Ok(())
}

fn entity_uuid(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> Option<uuid::Uuid> {
    let uuid = ctx
        .env
        .call_method(entity, "getUUID", "()Ljava/util/UUID;", &[])
        .ok()?
        .l()
        .ok()?;
    let text = ctx
        .env
        .call_method(&uuid, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let java_str = ctx.env.get_string((&text).into()).ok()?;
    uuid::Uuid::parse_str(&java_str.to_string_lossy()).ok()
}
