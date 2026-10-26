use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JClass, JObject, JString, JValue},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

pub mod world;

use crate::{
    interface::Module,
    jni_bridge,
    utils::{
        friends,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        settings,
    },
};

pub const SIZE_SETTING: &str = "Size";
pub const ALPHA_SETTING: &str = "Alpha";
pub const RANGE_SETTING: &str = "Range";
pub const STATIC_SIZE_SETTING: &str = "Static size";
pub const SHOW_SELF_SETTING: &str = "Show self";
pub const SHOW_FRIENDS_SETTING: &str = "Show friends";
pub const SHOW_DONATE_SETTING: &str = "Show team prefix";
pub const SHOW_ARMOR_SETTING: &str = "Show armor";
pub const SHOW_HAND_ITEMS_SETTING: &str = "Show hand items";
pub const HEALTH_MODE_SETTING: &str = "HP mode";
pub const FRIEND_COLOR_SETTING: &str = "Friend color";

pub const HEALTH_POINTS: &str = "Health";
pub const HEALTH_HEARTS: &str = "Hearts";
pub const HEALTH_PERCENT: &str = "Percent";

const SCREEN_MARGIN: f32 = 96.0;
const DEFAULT_FOV_Y_DEGREES: f64 = 70.0;
const VANILLA_HIDE_TEAM_PREFIX: &str = "xz_nt_hide";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthDisplayMode {
    Points,
    Hearts,
    Percent,
}

#[derive(Clone, Debug)]
pub struct NameTagsConfig {
    pub size: f32,
    pub alpha: f32,
    pub range: f32,
    pub static_size: bool,
    pub show_self: bool,
    pub show_friends: bool,
    pub show_donate: bool,
    pub show_armor: bool,
    pub show_hand_items: bool,
    pub health_mode: HealthDisplayMode,
    pub friend_color: [f32; 4],
}

impl Default for NameTagsConfig {
    fn default() -> Self {
        Self {
            size: 0.35,
            alpha: 1.0,
            range: 64.0,
            static_size: false,
            show_self: false,
            show_friends: true,
            show_donate: false,
            show_armor: true,
            show_hand_items: true,
            health_mode: HealthDisplayMode::Points,
            friend_color: [0.25, 0.85, 0.45, 1.0],
        }
    }
}

impl NameTagsConfig {
    pub fn from_module(module: &Module) -> Self {
        let defaults = Self::default();
        let health_mode =
            match settings::choice(module, HEALTH_MODE_SETTING, HEALTH_POINTS).as_str() {
                HEALTH_HEARTS => HealthDisplayMode::Hearts,
                HEALTH_PERCENT => HealthDisplayMode::Percent,
                _ => HealthDisplayMode::Points,
            };

        Self {
            size: finite_clamped(
                settings::slider(module, SIZE_SETTING, defaults.size),
                0.01,
                2.0,
                defaults.size,
            ),
            alpha: finite_clamped(
                settings::slider(module, ALPHA_SETTING, defaults.alpha),
                0.0,
                1.0,
                defaults.alpha,
            ),
            range: finite_clamped(
                settings::slider(module, RANGE_SETTING, defaults.range),
                4.0,
                256.0,
                defaults.range,
            ),
            static_size: settings::checkbox(module, STATIC_SIZE_SETTING, defaults.static_size),
            show_self: settings::checkbox(module, SHOW_SELF_SETTING, defaults.show_self),
            show_friends: settings::checkbox(module, SHOW_FRIENDS_SETTING, defaults.show_friends),
            show_donate: settings::checkbox(module, SHOW_DONATE_SETTING, defaults.show_donate),
            show_armor: settings::checkbox(module, SHOW_ARMOR_SETTING, defaults.show_armor),
            show_hand_items: settings::checkbox(
                module,
                SHOW_HAND_ITEMS_SETTING,
                defaults.show_hand_items,
            ),
            health_mode,
            friend_color: settings::color_picker(
                module,
                FRIEND_COLOR_SETTING,
                defaults.friend_color,
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NameTagEntry {
    pub entity_id: i32,
    pub name: String,
    pub prefix: String,
    pub health_text: String,
    pub health_color: [u8; 3],
    pub name_color: [u8; 3],
    pub screen_x: f32,
    pub screen_y: f32,
    pub distance: f32,
    pub scale: f32,
    pub items: Vec<NameTagItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameTagItemSlot {
    MainHand,
    OffHand,
    Head,
    Chest,
    Legs,
    Feet,
}

impl NameTagItemSlot {
    fn equipment_slot_name(self) -> Option<&'static str> {
        match self {
            Self::Head => Some("HEAD"),
            Self::Chest => Some("CHEST"),
            Self::Legs => Some("LEGS"),
            Self::Feet => Some("FEET"),
            Self::MainHand | Self::OffHand => None,
        }
    }

    fn hand_method(self) -> Option<&'static str> {
        match self {
            Self::MainHand => Some("getMainHandItem"),
            Self::OffHand => Some("getOffhandItem"),
            Self::Head | Self::Chest | Self::Legs | Self::Feet => None,
        }
    }

    fn seed_offset(self) -> i32 {
        match self {
            Self::MainHand => 0,
            Self::OffHand => 1,
            Self::Head => 2,
            Self::Chest => 3,
            Self::Legs => 4,
            Self::Feet => 5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NameTagItem {
    pub slot: NameTagItemSlot,
}

#[derive(Clone, Copy, Debug)]
pub struct NameTagItemIconRequest {
    pub entity_id: i32,
    pub slot: NameTagItemSlot,
    
    pub inventory_slot: Option<i32>,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct NameTagPreparedIcon {
    pub texture_id: u32,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub nearest_filter: bool,
    pub durability_bar: Option<NameTagDurabilityBar>,
}

#[derive(Clone, Copy, Debug)]
pub struct NameTagDurabilityBar {
    pub width: i32,
    pub color: [u8; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    pub position: Vec3d,
    pub forward: Vec3d,
    pub yaw_degrees: f64,
    pub pitch_degrees: f64,
    pub fov_y_degrees: f64,
}

pub struct ProjectionContext<'local> {
    pub game_renderer: JObject<'local>,
    pub vec3_class: JClass<'local>,
    pub camera: Option<CameraView>,
}

#[derive(Default)]
struct VanillaNameRenderState {
    team_visibility: HashMap<String, String>,
    temp_team_name: Option<String>,
    temp_players: HashSet<String>,
}

static VANILLA_NAME_RENDER_STATE: OnceLock<Mutex<VanillaNameRenderState>> = OnceLock::new();

fn vanilla_name_render_state() -> &'static Mutex<VanillaNameRenderState> {
    VANILLA_NAME_RENDER_STATE.get_or_init(|| Mutex::new(VanillaNameRenderState::default()))
}

pub fn restore_vanilla_player_names(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let mut state = vanilla_name_render_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.team_visibility.is_empty()
        && state.temp_team_name.is_none()
        && state.temp_players.is_empty()
    {
        return Ok(());
    }

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        *state = VanillaNameRenderState::default();
        return Ok(());
    };
    if !minecraft::has_player_and_world(&ctx) {
        *state = VanillaNameRenderState::default();
        return Ok(());
    }

    let scoreboard = level_scoreboard(&mut ctx)?;
    restore_vanilla_player_names_locked(&mut ctx, &scoreboard, &mut state)?;
    let _ = ctx.env.delete_local_ref(scoreboard);
    Ok(())
}

pub fn hide_vanilla_player_names_for_next_frame(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(());
    }

    let scoreboard = level_scoreboard(&mut ctx)?;
    let never = team_visibility_constant(ctx.env, "NEVER")?;
    let player_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let iterable = player_iterable(&mut ctx)?;
    let iterator = ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let mut state = vanilla_name_render_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut temp_team: Option<JObject<'_>> = None;

    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;

        if !ctx.env.is_instance_of(&entity, &player_class)?
            || ctx.env.is_same_object(&entity, &ctx.player)?
        {
            let _ = ctx.env.delete_local_ref(entity);
            continue;
        }

        let player_name = scoreboard_name(ctx.env, &entity)?;
        let team = scoreboard_players_team(ctx.env, &scoreboard, &player_name)?;
        if let Some(team) = team {
            let team_name = team_object_name(ctx.env, &team)?;
            if state.temp_team_name.as_deref() != Some(team_name.as_str()) {
                if !state.team_visibility.contains_key(&team_name) {
                    let visibility = team_visibility_name(ctx.env, &team)?;
                    state.team_visibility.insert(team_name.clone(), visibility);
                }
                set_team_visibility(ctx.env, &team, &never)?;
            }
            let _ = ctx.env.delete_local_ref(team);
        } else {
            if temp_team.is_none() {
                temp_team = ensure_temp_hide_team(&mut ctx, &scoreboard, &never, &mut state)?;
            }
            if let Some(team) = temp_team.as_ref() {
                if scoreboard_add_player_to_team(ctx.env, &scoreboard, &player_name, team)? {
                    state.temp_players.insert(player_name);
                }
            }
        }

        let _ = ctx.env.delete_local_ref(entity);
    }

    if let Some(team) = temp_team {
        let _ = ctx.env.delete_local_ref(team);
    }
    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);
    let _ = ctx.env.delete_local_ref(never);
    let _ = ctx.env.delete_local_ref(scoreboard);
    Ok(())
}

pub fn collect(
    env: &mut JNIEnv<'_>,
    config: &NameTagsConfig,
    viewport: [f32; 2],
) -> JniResult<Vec<NameTagEntry>> {
    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(Vec::new());
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(Vec::new());
    }

    let camera = match camera_view(&mut ctx) {
        Ok(camera) => Some(camera),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            None
        }
    };
    let projection = ProjectionContext {
        game_renderer: game_renderer(&mut ctx)?,
        vec3_class: jni_bridge::load_class(ctx.env, "net.minecraft.world.phys.Vec3")?,
        camera,
    };
    let player_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let player_team = match team_name(ctx.env, &ctx.player) {
        Ok(team) => team,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            None
        }
    };
    let partial_ticks = frame_partial_ticks(&mut ctx);
    let player_pos = entity_position(ctx.env, &ctx.player, partial_ticks)?;

    let iterable = player_iterable(&mut ctx)?;
    let iterator = ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let mut entries = Vec::new();
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;

        if ctx.env.is_instance_of(&entity, &player_class)? {
            if let Some(entry) = collect_player_tag(
                &mut ctx,
                &entity,
                config,
                viewport,
                &projection,
                player_pos,
                &player_team,
                partial_ticks,
            )? {
                entries.push(entry);
            }
        }

        let _ = ctx.env.delete_local_ref(entity);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);
    let _ = ctx.env.delete_local_ref(projection.game_renderer);

    entries.sort_by(|left, right| {
        right
            .distance
            .partial_cmp(&left.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(entries)
}

fn collect_player_tag<'env, 'local>(
    ctx: &mut MinecraftContext<'env, 'local>,
    entity: &JObject<'local>,
    config: &NameTagsConfig,
    viewport: [f32; 2],
    projection: &ProjectionContext<'local>,
    player_pos: Vec3d,
    player_team: &Option<String>,
    partial_ticks: f32,
) -> JniResult<Option<NameTagEntry>> {
    let is_self = ctx.env.is_same_object(entity, &ctx.player)?;
    if is_self && !config.show_self {
        return Ok(None);
    }

    if !minecraft::entity_is_alive(ctx, entity)? {
        return Ok(None);
    }

    let entity_pos = entity_position(ctx.env, entity, partial_ticks)?;
    let distance = player_pos.distance_to(entity_pos) as f32;
    if distance > config.range {
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

    let height = optional_float_method(ctx.env, entity, "getBbHeight", "()F", 1.8);
    let tag_position = Vec3d {
        x: entity_pos.x,
        y: entity_pos.y + f64::from(height) + 0.55,
        z: entity_pos.z,
    };

    let Some(screen) = project_to_screen(ctx.env, projection, tag_position, viewport)? else {
        return Ok(None);
    };

    if screen.x < -SCREEN_MARGIN
        || screen.y < -SCREEN_MARGIN
        || screen.x > viewport[0] + SCREEN_MARGIN
        || screen.y > viewport[1] + SCREEN_MARGIN
    {
        return Ok(None);
    }

    let health = optional_float_method(ctx.env, entity, "getHealth", "()F", 20.0)
        + optional_float_method(ctx.env, entity, "getAbsorptionAmount", "()F", 0.0);
    let max_health = optional_float_method(ctx.env, entity, "getMaxHealth", "()F", 20.0)
        + optional_float_method(ctx.env, entity, "getAbsorptionAmount", "()F", 0.0);
    let ratio = if max_health > 0.0 {
        (health / max_health).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut items = Vec::new();
    if config.show_hand_items {
        push_stack_from_method(ctx.env, entity, NameTagItemSlot::MainHand, &mut items)?;
        push_stack_from_method(ctx.env, entity, NameTagItemSlot::OffHand, &mut items)?;
    }
    if config.show_armor {
        collect_armor_items(ctx.env, entity, &mut items)?;
    }

    let prefix = if config.show_donate {
        sanitize_name_tag_text(&team_prefix(ctx.env, entity).unwrap_or_default())
    } else {
        String::new()
    };
    let name = match entity_name(ctx.env, entity) {
        Ok(name) => name,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            "Player".to_string()
        }
    };
    let entity_id = ctx.env.call_method(entity, "getId", "()I", &[])?.i()?;

    Ok(Some(NameTagEntry {
        entity_id,
        name,
        prefix,
        health_text: health_text(config.health_mode, health, max_health),
        health_color: health_color(ratio),
        name_color: [255, 255, 255],
        screen_x: screen.x,
        screen_y: screen.y,
        distance,
        scale: tag_scale(config, distance),
        items,
    }))
}

fn sanitize_name_tag_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut skip_format = false;
    for ch in input.chars() {
        if skip_format {
            skip_format = false;
            continue;
        }
        if ch == '\u{00A7}' || ch == '&' {
            skip_format = true;
            continue;
        }
        if ch.is_control() || ('\u{E000}'..='\u{F8FF}').contains(&ch) {
            continue;
        }
        out.push(ch);
    }
    out.trim().to_string()
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

fn level_scoreboard<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    ctx.env
        .call_method(
            &ctx.level,
            "getScoreboard",
            "()Lnet/minecraft/world/scores/Scoreboard;",
            &[],
        )?
        .l()
}

fn restore_vanilla_player_names_locked<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    scoreboard: &JObject<'local>,
    state: &mut VanillaNameRenderState,
) -> JniResult<()> {
    let temp_team_name = state.temp_team_name.take();
    let temp_players = state.temp_players.drain().collect::<Vec<_>>();
    if let Some(team_name) = temp_team_name {
        for player_name in temp_players {
            scoreboard_remove_player_from_team_if_current(
                ctx.env,
                scoreboard,
                &player_name,
                &team_name,
            )?;
        }
        if let Some(team) = scoreboard_team(ctx.env, scoreboard, &team_name)? {
            ctx.env.call_method(
                &scoreboard,
                "removePlayerTeam",
                "(Lnet/minecraft/world/scores/PlayerTeam;)V",
                &[JValue::Object(&team)],
            )?;
            let _ = ctx.env.delete_local_ref(team);
        }
    }

    for (team_name, visibility_name) in state.team_visibility.drain().collect::<Vec<_>>() {
        let Some(team) = scoreboard_team(ctx.env, scoreboard, &team_name)? else {
            continue;
        };
        let visibility = team_visibility_constant(ctx.env, &visibility_name)?;
        set_team_visibility(ctx.env, &team, &visibility)?;
        let _ = ctx.env.delete_local_ref(visibility);
        let _ = ctx.env.delete_local_ref(team);
    }

    Ok(())
}

fn ensure_temp_hide_team<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    scoreboard: &JObject<'local>,
    never: &JObject<'local>,
    state: &mut VanillaNameRenderState,
) -> JniResult<Option<JObject<'local>>> {
    if let Some(team_name) = state.temp_team_name.clone() {
        if let Some(team) = scoreboard_team(ctx.env, scoreboard, &team_name)? {
            set_team_visibility(ctx.env, &team, never)?;
            return Ok(Some(team));
        }
        state.temp_team_name = None;
    }

    for suffix in 0..32 {
        let team_name = if suffix == 0 {
            VANILLA_HIDE_TEAM_PREFIX.to_string()
        } else {
            format!("{VANILLA_HIDE_TEAM_PREFIX}{suffix}")
        };

        if let Some(existing) = scoreboard_team(ctx.env, scoreboard, &team_name)? {
            let _ = ctx.env.delete_local_ref(existing);
            continue;
        }

        let java_name = ctx.env.new_string(&team_name)?;
        let java_name = JObject::from(java_name);
        let team = ctx
            .env
            .call_method(
                scoreboard,
                "addPlayerTeam",
                "(Ljava/lang/String;)Lnet/minecraft/world/scores/PlayerTeam;",
                &[JValue::Object(&java_name)],
            )?
            .l()?;
        let _ = ctx.env.delete_local_ref(java_name);
        set_team_visibility(ctx.env, &team, never)?;
        state.temp_team_name = Some(team_name);
        return Ok(Some(team));
    }

    Ok(None)
}

fn scoreboard_team<'local>(
    env: &mut JNIEnv<'local>,
    scoreboard: &JObject<'local>,
    team_name: &str,
) -> JniResult<Option<JObject<'local>>> {
    let java_name = env.new_string(team_name)?;
    let java_name = JObject::from(java_name);
    let team = env
        .call_method(
            scoreboard,
            "getPlayerTeam",
            "(Ljava/lang/String;)Lnet/minecraft/world/scores/PlayerTeam;",
            &[JValue::Object(&java_name)],
        )?
        .l()?;
    let _ = env.delete_local_ref(java_name);

    if team.is_null() {
        Ok(None)
    } else {
        Ok(Some(team))
    }
}

fn scoreboard_players_team<'local>(
    env: &mut JNIEnv<'local>,
    scoreboard: &JObject<'local>,
    player_name: &str,
) -> JniResult<Option<JObject<'local>>> {
    let java_name = env.new_string(player_name)?;
    let java_name = JObject::from(java_name);
    let team = env
        .call_method(
            scoreboard,
            "getPlayersTeam",
            "(Ljava/lang/String;)Lnet/minecraft/world/scores/PlayerTeam;",
            &[JValue::Object(&java_name)],
        )?
        .l()?;
    let _ = env.delete_local_ref(java_name);

    if team.is_null() {
        Ok(None)
    } else {
        Ok(Some(team))
    }
}

fn scoreboard_add_player_to_team(
    env: &mut JNIEnv<'_>,
    scoreboard: &JObject<'_>,
    player_name: &str,
    team: &JObject<'_>,
) -> JniResult<bool> {
    let java_name = env.new_string(player_name)?;
    let java_name = JObject::from(java_name);
    let added = env
        .call_method(
            scoreboard,
            "addPlayerToTeam",
            "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z",
            &[JValue::Object(&java_name), JValue::Object(team)],
        )?
        .z()?;
    let _ = env.delete_local_ref(java_name);
    Ok(added)
}

fn scoreboard_remove_player_from_team_if_current<'local>(
    env: &mut JNIEnv<'local>,
    scoreboard: &JObject<'local>,
    player_name: &str,
    expected_team_name: &str,
) -> JniResult<()> {
    let Some(team) = scoreboard_players_team(env, scoreboard, player_name)? else {
        return Ok(());
    };
    let team_name = team_object_name(env, &team)?;
    let _ = env.delete_local_ref(team);
    if team_name != expected_team_name {
        return Ok(());
    }

    let java_name = env.new_string(player_name)?;
    let java_name = JObject::from(java_name);
    env.call_method(
        scoreboard,
        "removePlayerFromTeam",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&java_name)],
    )?;
    let _ = env.delete_local_ref(java_name);
    Ok(())
}

fn scoreboard_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<String> {
    let name = env
        .call_method(entity, "getScoreboardName", "()Ljava/lang/String;", &[])?
        .l()?;
    java_string(env, name)
}

fn team_object_name(env: &mut JNIEnv<'_>, team: &JObject<'_>) -> JniResult<String> {
    let name = env
        .call_method(team, "getName", "()Ljava/lang/String;", &[])?
        .l()?;
    java_string(env, name)
}

fn team_visibility_name(env: &mut JNIEnv<'_>, team: &JObject<'_>) -> JniResult<String> {
    let visibility = env
        .call_method(
            team,
            "getNameTagVisibility",
            "()Lnet/minecraft/world/scores/Team$Visibility;",
            &[],
        )?
        .l()?;
    let name = env
        .call_method(&visibility, "name", "()Ljava/lang/String;", &[])?
        .l()?;
    let name = java_string(env, name)?;
    let _ = env.delete_local_ref(visibility);
    Ok(name)
}

fn team_visibility_constant<'local>(
    env: &mut JNIEnv<'local>,
    name: &str,
) -> JniResult<JObject<'local>> {
    let visibility_class =
        jni_bridge::load_class(env, "net.minecraft.world.scores.Team$Visibility")?;
    env.get_static_field(
        visibility_class,
        name,
        "Lnet/minecraft/world/scores/Team$Visibility;",
    )?
    .l()
}

fn set_team_visibility(
    env: &mut JNIEnv<'_>,
    team: &JObject<'_>,
    visibility: &JObject<'_>,
) -> JniResult<()> {
    env.call_method(
        team,
        "setNameTagVisibility",
        "(Lnet/minecraft/world/scores/Team$Visibility;)V",
        &[JValue::Object(visibility)],
    )?;
    Ok(())
}

pub fn game_renderer<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    ctx.env
        .get_field(
            &ctx.minecraft,
            "gameRenderer",
            "Lnet/minecraft/client/renderer/GameRenderer;",
        )?
        .l()
}

pub fn camera_view(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<CameraView> {
    let game_renderer = ctx
        .env
        .get_field(
            &ctx.minecraft,
            "gameRenderer",
            "Lnet/minecraft/client/renderer/GameRenderer;",
        )?
        .l()?;
    let camera = ctx
        .env
        .call_method(
            &game_renderer,
            "mainCamera",
            "()Lnet/minecraft/client/Camera;",
            &[],
        )?
        .l()?;
    let position = ctx
        .env
        .get_field(&camera, "position", "Lnet/minecraft/world/phys/Vec3;")?
        .l()?;
    let yaw_degrees = ctx.env.get_field(&camera, "yRot", "F")?.f()? as f64;
    let pitch_degrees = ctx.env.get_field(&camera, "xRot", "F")?.f()? as f64;

    let result = CameraView {
        position: vec3_from_java(ctx.env, &position)?,
        forward: camera_forward(yaw_degrees, pitch_degrees),
        yaw_degrees,
        pitch_degrees,
        fov_y_degrees: ctx
            .env
            .call_method(&camera, "getFov", "()F", &[])?
            .f()?
            .max(1.0) as f64,
    };

    let _ = ctx.env.delete_local_ref(position);
    let _ = ctx.env.delete_local_ref(camera);
    let _ = ctx.env.delete_local_ref(game_renderer);
    Ok(result)
}

pub fn project_to_screen(
    env: &mut JNIEnv<'_>,
    projection: &ProjectionContext<'_>,
    point: Vec3d,
    viewport: [f32; 2],
) -> JniResult<Option<ScreenPoint>> {
    if let Some(camera) = &projection.camera {
        let relative = Vec3d {
            x: point.x - camera.position.x,
            y: point.y - camera.position.y,
            z: point.z - camera.position.z,
        };
        if dot(relative, camera.forward) <= 0.05 {
            return Ok(None);
        }
    }

    let java_point = env.new_object(
        &projection.vec3_class,
        "(DDD)V",
        &[
            JValue::Double(point.x),
            JValue::Double(point.y),
            JValue::Double(point.z),
        ],
    )?;
    let projected = match env.call_method(
        &projection.game_renderer,
        "projectPointToScreen",
        "(Lnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;",
        &[JValue::Object(&java_point)],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            let _ = env.delete_local_ref(java_point);
            return Ok(projection
                .camera
                .as_ref()
                .and_then(|camera| manual_project_to_screen(point, camera, viewport)));
        }
    };
    let ndc = vec3_from_java(env, &projected)?;
    let _ = env.delete_local_ref(projected);
    let _ = env.delete_local_ref(java_point);

    if !ndc.x.is_finite() || !ndc.y.is_finite() || !ndc.z.is_finite() {
        return Ok(None);
    }

    let width = viewport[0].max(1.0);
    let height = viewport[1].max(1.0);
    Ok(Some(ScreenPoint {
        x: ((ndc.x as f32 + 1.0) * 0.5) * width,
        y: ((1.0 - ndc.y as f32) * 0.5) * height,
    }))
}

pub fn manual_project_to_screen(
    point: Vec3d,
    camera: &CameraView,
    viewport: [f32; 2],
) -> Option<ScreenPoint> {
    let width = f64::from(viewport[0].max(1.0));
    let height = f64::from(viewport[1].max(1.0));
    let mut right = normalize(cross(
        camera.forward,
        Vec3d {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    ));
    if length_sqr(right) < f64::EPSILON {
        right = Vec3d {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        };
    }
    let up = normalize(cross(right, camera.forward));
    let relative = Vec3d {
        x: point.x - camera.position.x,
        y: point.y - camera.position.y,
        z: point.z - camera.position.z,
    };

    let depth = dot(relative, camera.forward);
    if depth <= 0.05 {
        return None;
    }

    let fov = if camera.fov_y_degrees.is_finite() {
        camera.fov_y_degrees.clamp(1.0, 170.0)
    } else {
        DEFAULT_FOV_Y_DEGREES
    };
    let focal = height * 0.5 / (fov.to_radians() * 0.5).tan();
    let x = width * 0.5 + dot(relative, right) * focal / depth;
    let y = height * 0.5 - dot(relative, up) * focal / depth;

    Some(ScreenPoint {
        x: x as f32,
        y: y as f32,
    })
}

fn collect_armor_items(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    items: &mut Vec<NameTagItem>,
) -> JniResult<()> {
    let slot_class = jni_bridge::load_class(env, "net.minecraft.world.entity.EquipmentSlot")?;
    for (slot_name, item_slot) in [
        ("HEAD", NameTagItemSlot::Head),
        ("CHEST", NameTagItemSlot::Chest),
        ("LEGS", NameTagItemSlot::Legs),
        ("FEET", NameTagItemSlot::Feet),
    ] {
        let slot = env
            .get_static_field(
                &slot_class,
                slot_name,
                "Lnet/minecraft/world/entity/EquipmentSlot;",
            )?
            .l()?;
        let stack = env
            .call_method(
                entity,
                "getItemBySlot",
                "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;",
                &[JValue::Object(&slot)],
            )?
            .l()?;
        if let Some(item) = stack_summary(env, &stack, item_slot)? {
            items.push(item);
        }
        let _ = env.delete_local_ref(stack);
        let _ = env.delete_local_ref(slot);
    }

    Ok(())
}

fn push_stack_from_method(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    slot: NameTagItemSlot,
    items: &mut Vec<NameTagItem>,
) -> JniResult<()> {
    let Some(method) = slot.hand_method() else {
        return Ok(());
    };
    let stack = match env.call_method(
        entity,
        method,
        "()Lnet/minecraft/world/item/ItemStack;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(());
        }
    };
    if let Some(item) = stack_summary(env, &stack, slot)? {
        items.push(item);
    }
    let _ = env.delete_local_ref(stack);
    Ok(())
}

fn stack_summary(
    env: &mut JNIEnv<'_>,
    stack: &JObject<'_>,
    slot: NameTagItemSlot,
) -> JniResult<Option<NameTagItem>> {
    if stack.is_null() || env.call_method(stack, "isEmpty", "()Z", &[])?.z()? {
        return Ok(None);
    }

    Ok(Some(NameTagItem { slot }))
}

fn entity_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<String> {
    let name = component_text_from_method(
        env,
        entity,
        "getName",
        "()Lnet/minecraft/network/chat/Component;",
    )?;
    Ok(strip_format_codes(&name))
}

fn team_prefix<'local>(env: &mut JNIEnv<'local>, entity: &JObject<'local>) -> Option<String> {
    let team = team_object(env, entity).ok().flatten()?;
    let prefix = optional_component_text_from_method(
        env,
        &team,
        "getPlayerPrefix",
        "()Lnet/minecraft/network/chat/Component;",
    )
    .or_else(|| {
        optional_component_text_from_method(
            env,
            &team,
            "getPrefix",
            "()Lnet/minecraft/network/chat/Component;",
        )
    })
    .map(|text| strip_format_codes(&text).trim().to_string())
    .unwrap_or_default();
    let _ = env.delete_local_ref(team);

    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

fn team_name<'local>(
    env: &mut JNIEnv<'local>,
    entity: &JObject<'local>,
) -> JniResult<Option<String>> {
    let Some(team) = team_object(env, entity)? else {
        return Ok(None);
    };
    let name = match env.call_method(&team, "getName", "()Ljava/lang/String;", &[]) {
        Ok(value) => java_string(env, value.l()?)?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            String::new()
        }
    };
    let _ = env.delete_local_ref(team);

    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

fn team_object<'local>(
    env: &mut JNIEnv<'local>,
    entity: &JObject<'local>,
) -> JniResult<Option<JObject<'local>>> {
    let value = match env.call_method(
        entity,
        "getTeam",
        "()Lnet/minecraft/world/scores/PlayerTeam;",
        &[],
    ) {
        Ok(value) => value,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    let team = value.l()?;
    if team.is_null() {
        Ok(None)
    } else {
        Ok(Some(team))
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
            let result = vec3_from_java(env, &position);
            let _ = env.delete_local_ref(position);
            result
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

fn component_text_from_method(
    env: &mut JNIEnv<'_>,
    object: &JObject<'_>,
    method: &str,
    signature: &str,
) -> JniResult<String> {
    let component = env.call_method(object, method, signature, &[])?.l()?;
    component_text(env, component)
}

fn optional_component_text_from_method(
    env: &mut JNIEnv<'_>,
    object: &JObject<'_>,
    method: &str,
    signature: &str,
) -> Option<String> {
    let component = match env.call_method(object, method, signature, &[]) {
        Ok(value) => value.l().ok()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return None;
        }
    };
    match component_text(env, component) {
        Ok(text) => Some(text),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            None
        }
    }
}

fn component_text(env: &mut JNIEnv<'_>, component: JObject<'_>) -> JniResult<String> {
    if component.is_null() {
        return Ok(String::new());
    }

    let string = env
        .call_method(&component, "getString", "()Ljava/lang/String;", &[])?
        .l()?;
    let text = java_string(env, string)?;
    let _ = env.delete_local_ref(component);
    Ok(text)
}

fn java_string(env: &mut JNIEnv<'_>, string: JObject<'_>) -> JniResult<String> {
    if string.is_null() {
        return Ok(String::new());
    }

    let string = JString::from(string);
    let text = env.get_string(&string)?.to_string_lossy().into_owned();
    let _ = env.delete_local_ref(JObject::from(string));
    Ok(text)
}

fn vec3_from_java(env: &mut JNIEnv<'_>, vector: &JObject<'_>) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: env.get_field(vector, "x", "D")?.d()?,
        y: env.get_field(vector, "y", "D")?.d()?,
        z: env.get_field(vector, "z", "D")?.d()?,
    })
}

fn optional_float_method(
    env: &mut JNIEnv<'_>,
    object: &JObject<'_>,
    method: &str,
    signature: &str,
    default: f32,
) -> f32 {
    match env.call_method(object, method, signature, &[]) {
        Ok(value) => value.f().unwrap_or(default),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            default
        }
    }
}

fn health_text(mode: HealthDisplayMode, health: f32, max_health: f32) -> String {
    match mode {
        HealthDisplayMode::Points => format!("{health:.1}"),
        HealthDisplayMode::Hearts => {
            let hearts = (health / 2.0).round() as i32;
            let max_hearts = (max_health.max(1.0) / 2.0).round() as i32;
            format!("{hearts}/{max_hearts}")
        }
        HealthDisplayMode::Percent => {
            let percent = if max_health > 0.0 {
                health / max_health * 100.0
            } else {
                0.0
            };
            format!("{percent:.0}%")
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

fn tag_scale(config: &NameTagsConfig, distance: f32) -> f32 {
    let size = (config.size / 0.35).clamp(0.2, 4.0);
    if config.static_size {
        return size;
    }

    let distance_factor = 1.15 - (distance / config.range.max(1.0)).clamp(0.0, 1.0) * 0.45;
    (size * distance_factor).clamp(0.45, 3.0)
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
        ch.to_ascii_lowercase(),
        '0'..='9' | 'a'..='f' | 'k'..='o' | 'r'
    )
}

pub fn prepare_item_icons(
    env: &mut JNIEnv<'_>,
    requests: &[NameTagItemIconRequest],
) -> JniResult<Vec<NameTagPreparedIcon>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(Vec::new());
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(Vec::new());
    }

    let window = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getWindow",
            "()Lcom/mojang/blaze3d/platform/Window;",
            &[],
        )?
        .l()?;
    let gui_scale = ctx
        .env
        .call_method(&window, "getGuiScale", "()I", &[])?
        .i()?
        .max(1) as f32;
    let game_renderer = game_renderer(&mut ctx)?;
    let resolver = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getItemModelResolver",
            "()Lnet/minecraft/client/renderer/item/ItemModelResolver;",
            &[],
        )?
        .l()?;
    let state_class = jni_bridge::load_class(
        ctx.env,
        "net.minecraft.client.renderer.item.TrackingItemStackRenderState",
    )?;
    let display_context_class =
        jni_bridge::load_class(ctx.env, "net.minecraft.world.item.ItemDisplayContext")?;
    let display_gui = ctx
        .env
        .get_static_field(
            &display_context_class,
            "GUI",
            "Lnet/minecraft/world/item/ItemDisplayContext;",
        )?
        .l()?;
    let hash_set_class = jni_bridge::load_class(ctx.env, "java.util.HashSet")?;
    let identities = ctx.env.new_object(&hash_set_class, "()V", &[])?;

    struct PendingIcon<'local> {
        state: JObject<'local>,
        x: f32,
        y: f32,
        size: f32,
        durability_bar: Option<NameTagDurabilityBar>,
    }

    let mut pending = Vec::new();
    for request in requests.iter().take(96) {
        if !request.x.is_finite()
            || !request.y.is_finite()
            || !request.size.is_finite()
            || request.size <= 1.0
        {
            continue;
        }

        let Some(entity) = ctx.entity_by_id(request.entity_id)? else {
            continue;
        };
        let stack = if let Some(inv_slot) = request.inventory_slot {
            inventory_stack(&mut ctx, inv_slot)?
        } else {
            stack_for_slot(ctx.env, &entity, request.slot)?
        };
        if stack.is_null() || ctx.env.call_method(&stack, "isEmpty", "()Z", &[])?.z()? {
            let _ = ctx.env.delete_local_ref(stack);
            let _ = ctx.env.delete_local_ref(entity);
            continue;
        }

        let state = ctx.env.new_object(&state_class, "()V", &[])?;
        let seed = request.entity_id
            + request
                .inventory_slot
                .unwrap_or_else(|| request.slot.seed_offset());
        ctx.env.call_method(
            &resolver,
            "updateForTopItem",
            "(Lnet/minecraft/client/renderer/item/ItemStackRenderState;Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemDisplayContext;Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/ItemOwner;I)V",
            &[
                JValue::Object(&state),
                JValue::Object(&stack),
                JValue::Object(&display_gui),
                JValue::Object(&ctx.level),
                JValue::Object(&entity),
                JValue::Int(seed),
            ],
        )?;
        if ctx.env.call_method(&state, "isEmpty", "()Z", &[])?.z()? {
            let _ = ctx.env.delete_local_ref(state);
            let _ = ctx.env.delete_local_ref(stack);
            let _ = ctx.env.delete_local_ref(entity);
            continue;
        }

        let identity = ctx
            .env
            .call_method(&state, "getModelIdentity", "()Ljava/lang/Object;", &[])?
            .l()?;
        ctx.env.call_method(
            &identities,
            "add",
            "(Ljava/lang/Object;)Z",
            &[JValue::Object(&identity)],
        )?;
        let _ = ctx.env.delete_local_ref(identity);

        pending.push(PendingIcon {
            state,
            x: request.x,
            y: request.y,
            size: request.size,
            durability_bar: stack_durability_bar(ctx.env, &stack),
        });
        let _ = ctx.env.delete_local_ref(stack);
        let _ = ctx.env.delete_local_ref(entity);
    }

    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let gui_renderer = ctx
        .env
        .get_field(
            &game_renderer,
            "guiRenderer",
            "Lnet/minecraft/client/gui/render/GuiRenderer;",
        )?
        .l()?;
    let slot_texture_size = (16.0 * gui_scale).round().max(16.0) as i32;
    let item_atlas = ctx
        .env
        .call_method(
            &gui_renderer,
            "prepareItemAtlas",
            "(Ljava/util/Set;I)Lnet/minecraft/client/gui/render/GuiItemAtlas;",
            &[JValue::Object(&identities), JValue::Int(slot_texture_size)],
        )?
        .l()?;

    let mut icons = Vec::with_capacity(pending.len());
    for icon in pending {
        let slot_view = ctx.env.call_method(
            &item_atlas,
            "getOrUpdate",
            "(Lnet/minecraft/client/renderer/item/TrackingItemStackRenderState;)Lnet/minecraft/client/gui/render/GuiItemAtlas$SlotView;",
            &[JValue::Object(&icon.state)],
        )?.l()?;
        if slot_view.is_null() {
            let _ = ctx.env.delete_local_ref(icon.state);
            continue;
        }

        let texture_view = ctx
            .env
            .call_method(
                &slot_view,
                "textureView",
                "()Lcom/mojang/blaze3d/textures/GpuTextureView;",
                &[],
            )?
            .l()?;
        let texture = ctx
            .env
            .call_method(
                &texture_view,
                "texture",
                "()Lcom/mojang/blaze3d/textures/GpuTexture;",
                &[],
            )?
            .l()?;
        let texture_id = ctx.env.call_method(&texture, "glId", "()I", &[])?.i()?;
        if texture_id > 0 {
            icons.push(NameTagPreparedIcon {
                texture_id: texture_id as u32,
                x: icon.x,
                y: icon.y,
                size: icon.size,
                u0: ctx.env.call_method(&slot_view, "u0", "()F", &[])?.f()?,
                v0: ctx.env.call_method(&slot_view, "v0", "()F", &[])?.f()?,
                u1: ctx.env.call_method(&slot_view, "u1", "()F", &[])?.f()?,
                v1: ctx.env.call_method(&slot_view, "v1", "()F", &[])?.f()?,
                nearest_filter: false,
                durability_bar: icon.durability_bar,
            });
        }

        let _ = ctx.env.delete_local_ref(texture);
        let _ = ctx.env.delete_local_ref(texture_view);
        let _ = ctx.env.delete_local_ref(slot_view);
        let _ = ctx.env.delete_local_ref(icon.state);
    }

    let _ = ctx.env.delete_local_ref(item_atlas);
    let _ = ctx.env.delete_local_ref(gui_renderer);
    let _ = ctx.env.delete_local_ref(identities);
    Ok(icons)
}

fn stack_for_slot<'local>(
    env: &mut JNIEnv<'local>,
    entity: &JObject<'local>,
    slot: NameTagItemSlot,
) -> JniResult<JObject<'local>> {
    if let Some(method) = slot.hand_method() {
        return env
            .call_method(
                entity,
                method,
                "()Lnet/minecraft/world/item/ItemStack;",
                &[],
            )?
            .l();
    }

    let slot_class = jni_bridge::load_class(env, "net.minecraft.world.entity.EquipmentSlot")?;
    let java_slot = env
        .get_static_field(
            &slot_class,
            slot.equipment_slot_name().unwrap_or("HEAD"),
            "Lnet/minecraft/world/entity/EquipmentSlot;",
        )?
        .l()?;
    let stack = env
        .call_method(
            entity,
            "getItemBySlot",
            "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;",
            &[JValue::Object(&java_slot)],
        )?
        .l();
    let _ = env.delete_local_ref(java_slot);
    stack
}

fn inventory_stack<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    slot: i32,
) -> JniResult<JObject<'local>> {
    let inventory = ctx
        .env
        .call_method(
            &ctx.player,
            "getInventory",
            "()Lnet/minecraft/world/entity/player/Inventory;",
            &[],
        )?
        .l()?;
    let stack = ctx
        .env
        .call_method(
            &inventory,
            "getItem",
            "(I)Lnet/minecraft/world/item/ItemStack;",
            &[JValue::Int(slot)],
        )?
        .l();
    let _ = ctx.env.delete_local_ref(inventory);
    stack
}

fn stack_durability_bar(env: &mut JNIEnv<'_>, stack: &JObject<'_>) -> Option<NameTagDurabilityBar> {
    let result = (|| -> JniResult<Option<NameTagDurabilityBar>> {
        if !env.call_method(stack, "isBarVisible", "()Z", &[])?.z()? {
            return Ok(None);
        }

        let width = env.call_method(stack, "getBarWidth", "()I", &[])?.i()?;
        let color = env.call_method(stack, "getBarColor", "()I", &[])?.i()? as u32;
        Ok(Some(NameTagDurabilityBar {
            width: width.clamp(0, 13),
            color: [
                ((color >> 16) & 0xFF) as u8,
                ((color >> 8) & 0xFF) as u8,
                (color & 0xFF) as u8,
            ],
        }))
    })();

    match result {
        Ok(bar) => bar,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            None
        }
    }
}

fn finite_clamped(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

fn dot(left: Vec3d, right: Vec3d) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn cross(left: Vec3d, right: Vec3d) -> Vec3d {
    Vec3d {
        x: left.y * right.z - left.z * right.y,
        y: left.z * right.x - left.x * right.z,
        z: left.x * right.y - left.y * right.x,
    }
}

fn length_sqr(value: Vec3d) -> f64 {
    dot(value, value)
}

fn normalize(value: Vec3d) -> Vec3d {
    let length = length_sqr(value).sqrt();
    if length <= f64::EPSILON {
        Vec3d::default()
    } else {
        Vec3d {
            x: value.x / length,
            y: value.y / length,
            z: value.z / length,
        }
    }
}

fn camera_forward(yaw_degrees: f64, pitch_degrees: f64) -> Vec3d {
    let yaw = yaw_degrees.to_radians();
    let pitch = pitch_degrees.to_radians();
    let pitch_cos = pitch.cos();
    Vec3d {
        x: -yaw.sin() * pitch_cos,
        y: -pitch.sin(),
        z: yaw.cos() * pitch_cos,
    }
}

pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
}
