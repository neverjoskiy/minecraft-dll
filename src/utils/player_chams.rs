use std::sync::{LazyLock, Mutex};

use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
};

use crate::{
    jni_bridge,
    utils::{
        friends,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        player_esp::PlayerEspConfig,
    },
};

const CHAMS_TEAM_PREFIX: &str = "xz_chams_";

#[derive(Clone, Debug)]
struct TrackedEntity {
    entity_id: i32,
    scoreboard_name: String,
    had_invisible: bool,
    had_glowing: bool,
    previous_team: Option<String>,
}

static TRACKED_ENTITIES: LazyLock<Mutex<Vec<TrackedEntity>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn apply_frame(env: &mut JNIEnv<'_>, config: &PlayerEspConfig) -> JniResult<()> {
    if !config.needs_chams() {
        clear_all(env)?;
        return Ok(());
    }

    clear_all(env)?;

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };

    if !minecraft::has_player_and_world(&ctx) {
        return Ok(());
    }

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

    let mut active = Vec::new();
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;

        if ctx.env.is_instance_of(&entity, &player_class)? {
            match apply_to_entity(
                &mut ctx,
                &entity,
                config,
                player_pos,
                &player_team,
                partial_ticks,
            ) {
                Ok(Some(tracked)) => active.push(tracked),
                Ok(None) => {}
                Err(_) => jni_bridge::clear_pending_exception(ctx.env),
            }
        }

        let _ = ctx.env.delete_local_ref(entity);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);

    if let Ok(mut cache) = TRACKED_ENTITIES.lock() {
        *cache = active;
    }
    Ok(())
}

pub fn clear_all(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let tracked = TRACKED_ENTITIES
        .lock()
        .map(|mut cache| std::mem::take(&mut *cache))
        .unwrap_or_default();
    if tracked.is_empty() {
        return Ok(());
    }

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(());
    };
    let scoreboard = level_scoreboard(&mut ctx)?;

    for entry in tracked {
        if let Some(entity) = ctx.entity_by_id(entry.entity_id)? {
            restore_entity(&mut ctx, &scoreboard, &entity, &entry)?;
            let _ = ctx.env.delete_local_ref(entity);
        }
    }

    let _ = ctx.env.delete_local_ref(scoreboard);
    Ok(())
}

fn apply_to_entity(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    config: &PlayerEspConfig,
    player_pos: Vec3d,
    player_team: &Option<String>,
    partial_ticks: f32,
) -> JniResult<Option<TrackedEntity>> {
    if !config.show_chams {
        return Ok(None);
    }

    let is_self = ctx.env.is_same_object(entity, &ctx.player)?;
    if is_self && !config.show_self {
        return Ok(None);
    }
    if !minecraft::entity_is_alive(ctx, entity)? {
        return Ok(None);
    }
    
    
    if let Some(uuid) = entity_uuid(ctx, entity)
        && crate::world_render::skip_player(uuid)
    {
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
    if scoreboard_name.is_empty() {
        return Ok(None);
    }
    let explicit_friend = friends::is_friend(&scoreboard_name);
    if (same_team || explicit_friend) && !config.show_friends {
        return Ok(None);
    }

    let had_invisible = entity_flag(ctx.env, entity, "isInvisible", "()Z")?;
    let had_glowing = entity_flag(ctx.env, entity, "isCurrentlyGlowing", "()Z")?;
    let previous_team = current_team_name(ctx.env, entity)?;

    let rgb = if explicit_friend || same_team {
        color_picker_rgb(config.friend_color)
    } else {
        color_picker_rgb(config.box_color)
    };

    let scoreboard = level_scoreboard(ctx)?;
    assign_outline_color(ctx, &scoreboard, &scoreboard_name, rgb)?;

    
    set_entity_glowing(ctx.env, entity, true)?;
    set_entity_invisible(ctx.env, entity, true)?;

    let entity_id = ctx.env.call_method(entity, "getId", "()I", &[])?.i()?;
    let _ = ctx.env.delete_local_ref(scoreboard);

    Ok(Some(TrackedEntity {
        entity_id,
        scoreboard_name,
        had_invisible,
        had_glowing,
        previous_team,
    }))
}

fn restore_entity<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    scoreboard: &JObject<'local>,
    entity: &JObject<'local>,
    tracked: &TrackedEntity,
) -> JniResult<()> {
    set_entity_glowing(ctx.env, entity, tracked.had_glowing)?;
    set_entity_invisible(ctx.env, entity, tracked.had_invisible)?;

    if let Some(team_name) = tracked.previous_team.as_ref() {
        if let Some(team) = scoreboard_team(ctx.env, scoreboard, team_name)? {
            let player_jstr = jni_string(ctx.env, &tracked.scoreboard_name)?;
            let _ = ctx.env.call_method(
                scoreboard,
                "addPlayerToTeam",
                "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z",
                &[JValue::Object(&player_jstr), JValue::Object(&team)],
            );
            let _ = ctx.env.delete_local_ref(team);
        }
    } else {
        remove_player_from_chams_teams(ctx.env, scoreboard, &tracked.scoreboard_name)?;
    }
    Ok(())
}

fn assign_outline_color<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    scoreboard: &JObject<'local>,
    player_name: &str,
    rgb: [u8; 3],
) -> JniResult<()> {
    remove_player_from_chams_teams(ctx.env, scoreboard, player_name)?;

    let team_name = format!("{CHAMS_TEAM_PREFIX}{}", team_suffix_for_rgb(rgb));
    let team = ensure_team(ctx, scoreboard, &team_name, rgb)?;
    let player_jstr = jni_string(ctx.env, player_name)?;
    ctx.env.call_method(
        scoreboard,
        "addPlayerToTeam",
        "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)Z",
        &[JValue::Object(&player_jstr), JValue::Object(&team)],
    )?;
    let _ = ctx.env.delete_local_ref(team);
    Ok(())
}

fn ensure_team<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    scoreboard: &JObject<'local>,
    team_name: &str,
    rgb: [u8; 3],
) -> JniResult<JObject<'local>> {
    if let Some(team) = scoreboard_team(ctx.env, scoreboard, team_name)? {
        let formatting = chat_formatting_for_rgb(ctx.env, rgb)?;
        set_team_color(ctx.env, &team, &formatting)?;
        let _ = ctx.env.delete_local_ref(formatting);
        return Ok(team);
    }

    let team_jstr = jni_string(ctx.env, team_name)?;
    let team = ctx
        .env
        .call_method(
            scoreboard,
            "addPlayerTeam",
            "(Ljava/lang/String;)Lnet/minecraft/world/scores/PlayerTeam;",
            &[JValue::Object(&team_jstr)],
        )?
        .l()?;
    let formatting = chat_formatting_for_rgb(ctx.env, rgb)?;
    set_team_color(ctx.env, &team, &formatting)?;
    let _ = ctx.env.delete_local_ref(formatting);
    ctx.env.call_method(
        &team,
        "setSeeFriendlyInvisibles",
        "(Z)V",
        &[JValue::Bool(false.into())],
    )?;
    Ok(team)
}

fn chat_formatting_for_rgb<'local>(
    env: &mut JNIEnv<'local>,
    rgb: [u8; 3],
) -> JniResult<JObject<'local>> {
    let field = if rgb[0] > 200 && rgb[1] < 120 && rgb[2] < 120 {
        "RED"
    } else if rgb[1] > 200 && rgb[0] < 120 && rgb[2] < 120 {
        "GREEN"
    } else if rgb[2] > 200 && rgb[0] < 120 && rgb[1] < 120 {
        "BLUE"
    } else if rgb[0] > 200 && rgb[1] > 200 && rgb[2] < 120 {
        "YELLOW"
    } else if rgb[0] > 200 && rgb[1] > 120 && rgb[2] < 120 {
        "GOLD"
    } else if rgb[0] > 200 && rgb[1] < 120 && rgb[2] > 200 {
        "LIGHT_PURPLE"
    } else if rgb[0] < 120 && rgb[1] > 200 && rgb[2] > 200 {
        "AQUA"
    } else {
        "WHITE"
    };
    let class = jni_bridge::load_class(env, "net.minecraft.ChatFormatting")?;
    env.get_static_field(&class, field, "Lnet/minecraft/ChatFormatting;")
        .and_then(|value| value.l())
}

fn remove_player_from_chams_teams(
    env: &mut JNIEnv<'_>,
    scoreboard: &JObject<'_>,
    player_name: &str,
) -> JniResult<()> {
    let teams = env
        .call_method(
            scoreboard,
            "getPlayerTeams",
            "()Ljava/util/Collection;",
            &[],
        )?
        .l()?;
    let iterator = env
        .call_method(&teams, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let player_jstr = jni_string(env, player_name)?;

    while env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let team = env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        let name_obj = env
            .call_method(&team, "getName", "()Ljava/lang/String;", &[])?
            .l()?;
        let name = java_string(env, name_obj)?;
        if name.starts_with(CHAMS_TEAM_PREFIX) {
            let _ = env.call_method(
                scoreboard,
                "removePlayerFromTeam",
                "(Ljava/lang/String;Lnet/minecraft/world/scores/PlayerTeam;)V",
                &[JValue::Object(&player_jstr), JValue::Object(&team)],
            );
        }
        let _ = env.delete_local_ref(team);
    }

    let _ = env.delete_local_ref(iterator);
    let _ = env.delete_local_ref(teams);
    Ok(())
}

fn set_team_color(
    env: &mut JNIEnv<'_>,
    team: &JObject<'_>,
    formatting: &JObject<'_>,
) -> JniResult<()> {
    env.call_method(
        team,
        "setColor",
        "(Lnet/minecraft/ChatFormatting;)V",
        &[JValue::Object(formatting)],
    )?;
    Ok(())
}

fn team_suffix_for_rgb(rgb: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn scoreboard_team<'local>(
    env: &mut JNIEnv<'local>,
    scoreboard: &JObject<'local>,
    team_name: &str,
) -> JniResult<Option<JObject<'local>>> {
    let name = jni_string(env, team_name)?;
    let team = env
        .call_method(
            scoreboard,
            "getPlayerTeam",
            "(Ljava/lang/String;)Lnet/minecraft/world/scores/PlayerTeam;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    if team.is_null() {
        Ok(None)
    } else {
        Ok(Some(team))
    }
}

fn current_team_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<Option<String>> {
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
    let name_obj = env
        .call_method(&team, "getName", "()Ljava/lang/String;", &[])?
        .l()?;
    let name = java_string(env, name_obj)?;
    let _ = env.delete_local_ref(team);
    if name.starts_with(CHAMS_TEAM_PREFIX) {
        Ok(None)
    } else {
        Ok(Some(name))
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

fn set_entity_glowing(env: &mut JNIEnv<'_>, entity: &JObject<'_>, glowing: bool) -> JniResult<()> {
    env.call_method(
        entity,
        "setGlowingTag",
        "(Z)V",
        &[JValue::Bool(glowing.into())],
    )?;
    Ok(())
}

fn set_entity_invisible(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    invisible: bool,
) -> JniResult<()> {
    env.call_method(
        entity,
        "setInvisible",
        "(Z)V",
        &[JValue::Bool(invisible.into())],
    )?;
    Ok(())
}

fn entity_flag(
    env: &mut JNIEnv<'_>,
    entity: &JObject<'_>,
    method: &str,
    sig: &str,
) -> JniResult<bool> {
    Ok(env.call_method(entity, method, sig, &[])?.z()?)
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
    let name_obj = env
        .call_method(&team, "getName", "()Ljava/lang/String;", &[])?
        .l()?;
    let name = java_string(env, name_obj)?;
    let _ = env.delete_local_ref(team);
    Ok(Some(name))
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

fn color_picker_rgb(color: [f32; 4]) -> [u8; 3] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn jni_string<'local>(env: &mut JNIEnv<'local>, value: &str) -> JniResult<JObject<'local>> {
    Ok(JObject::from(env.new_string(value)?))
}

fn java_string(env: &mut JNIEnv<'_>, object: JObject<'_>) -> JniResult<String> {
    if object.is_null() {
        return Ok(String::new());
    }
    let text = env
        .get_string((&object).into())?
        .to_string_lossy()
        .into_owned();
    let _ = env.delete_local_ref(object);
    Ok(text)
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
