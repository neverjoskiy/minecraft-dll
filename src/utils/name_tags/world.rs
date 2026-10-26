use jni::{JNIEnv, errors::Result as JniResult, objects::{JObject, JValue}};
use std::sync::Mutex;

use crate::{
    jni_bridge,
    utils::{
        friends,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
    },
    world_render::geometry,
};

use super::{HealthDisplayMode, NameTagsConfig, NameTagItem, NameTagItemSlot};

static WORLD_CONFIG: Mutex<Option<NameTagsConfig>> = Mutex::new(None);

pub fn set_world_config(config: Option<NameTagsConfig>) {
    if let Ok(mut slot) = WORLD_CONFIG.lock() {
        *slot = config;
    }
}


fn health_color(ratio: f32) -> [u8; 3] {
    if ratio > 0.75 { [0, 255, 0] } else if ratio > 0.5 { [255, 255, 0] } else if ratio > 0.25 { [255, 170, 0] } else { [255, 85, 85] }
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
            let percent = if max_health > 0.0 { health / max_health * 100.0 } else { 0.0 };
            format!("{percent:.0}%")
        }
    }
}
fn tag_scale(config: &NameTagsConfig, distance: f32) -> f32 {
    let size = (config.size / 0.35).clamp(0.2, 4.0);
    if config.static_size { size } else {
        let distance_factor = 1.15 - (distance / config.range.max(1.0)).clamp(0.0, 1.0) * 0.45;
        (size * distance_factor).clamp(0.45, 3.0)
    }
}
fn strip_format_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if (ch == '&' || ch as u32 == 0x00A7) && chars.peek().is_some_and(|next| is_format_code(*next)) { let _ = chars.next(); continue; }
        result.push(ch);
    }
    result
}
fn is_format_code(ch: char) -> bool {
    matches!(ch.to_ascii_lowercase(), '0'..='9' | 'a'..='f' | 'k'..='o' | 'r')
}
fn sanitize_name_tag_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut skip = false;
    for ch in input.chars() {
        if skip { skip = false; continue; }
        if ch == '\u{00A7}' || ch == '&' { skip = true; continue; }
        if ch.is_control() || ('\u{E000}'..='\u{F8FF}').contains(&ch) { continue; }
        out.push(ch);
    }
    out.trim().to_string()
}
fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().map(|ch| if ch == ' ' {0.32} else if ch.is_ascii_uppercase() || ch.is_ascii_digit() {0.62} else if ch.is_ascii() {0.54} else {0.74}).sum::<f32>() * font_size
}
fn optional_float(env: &mut JNIEnv<'_>, obj: &JObject<'_>, method: &str, default: f32) -> f32 {
    match env.call_method(obj, method, "()F", &[]) { Ok(v) => v.f().unwrap_or(default), Err(_) => { jni_bridge::clear_pending_exception(env); default } }
}
fn optional_component_text(env: &mut JNIEnv<'_>, obj: &JObject<'_>, method: &str, sig: &str) -> Option<String> {
    let comp = match env.call_method(obj, method, sig, &[]) { Ok(v) => v.l().ok()?, Err(_) => { jni_bridge::clear_pending_exception(env); return None; } };
    let s = env.call_method(&comp, "getString", "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
    let txt = env.get_string((&s).into()).ok()?.to_string_lossy().into_owned();
    let _ = env.delete_local_ref(comp);
    Some(txt)
}
fn component_text(env: &mut JNIEnv<'_>, obj: &JObject<'_>, method: &str, sig: &str) -> JniResult<String> {
    let comp = env.call_method(obj, method, sig, &[])?.l()?;
    let s = env.call_method(&comp, "getString", "()Ljava/lang/String;", &[])?.l()?;
    let txt = if s.is_null() { String::new() } else { let r = env.get_string((&s).into())?.to_string_lossy().into_owned(); let _ = env.delete_local_ref(s); r };
    let _ = env.delete_local_ref(comp);
    Ok(txt)
}
fn entity_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<String> {
    let name = component_text(env, entity, "getName", "()Lnet/minecraft/network/chat/Component;")?;
    Ok(strip_format_codes(&name))
}
fn team_prefix(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> Option<String> {
    let team = (|| -> JniResult<JObject<'_>> { env.call_method(entity, "getTeam", "()Lnet/minecraft/world/scores/PlayerTeam;", &[])?.l() })().ok()?;
    if team.is_null() { return None; }
    let prefix = optional_component_text(env, &team, "getPlayerPrefix", "()Lnet/minecraft/network/chat/Component;")
        .or_else(|| optional_component_text(env, &team, "getPrefix", "()Lnet/minecraft/network/chat/Component;"))
        .map(|t| strip_format_codes(&t).trim().to_string()).unwrap_or_default();
    let _ = env.delete_local_ref(team);
    if prefix.is_empty() { None } else { Some(prefix) }
}
fn team_name(env: &mut JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<Option<String>> {
    let team = match env.call_method(entity, "getTeam", "()Lnet/minecraft/world/scores/PlayerTeam;", &[]) { Ok(v) => v.l()?, Err(_) => { jni_bridge::clear_pending_exception(env); return Ok(None); } };
    if team.is_null() { return Ok(None); }
    let name = match env.call_method(&team, "getName", "()Ljava/lang/String;", &[]) { Ok(v) => { let s = v.l()?; if s.is_null() { None } else { let t = env.get_string((&s).into())?.to_string_lossy().into_owned(); let _ = env.delete_local_ref(s); Some(t) } }, Err(_) => { jni_bridge::clear_pending_exception(env); None } };
    let _ = env.delete_local_ref(team);
    Ok(name)
}
fn entity_position(env: &mut JNIEnv<'_>, entity: &JObject<'_>, partial: f32) -> JniResult<Vec3d> {
    match env.call_method(entity, "getPosition", "(F)Lnet/minecraft/world/phys/Vec3;", &[JValue::Float(partial)]) {
        Ok(v) => { let pos = v.l()?; let res = Vec3d { x: env.get_field(&pos, "x", "D")?.d()?, y: env.get_field(&pos, "y", "D")?.d()?, z: env.get_field(&pos, "z", "D")?.d()? }; let _ = env.delete_local_ref(pos); Ok(res) }
        Err(_) => { jni_bridge::clear_pending_exception(env); Ok(Vec3d { x: env.call_method(entity, "getX", "()D", &[])?.d()?, y: env.call_method(entity, "getY", "()D", &[])?.d()?, z: env.call_method(entity, "getZ", "()D", &[])?.d()? }) }
    }
}
fn frame_partial_ticks(ctx: &mut MinecraftContext<'_, '_>) -> f32 {
    let partial = (|| -> JniResult<f32> { let dt = ctx.env.call_method(&ctx.minecraft, "getDeltaTracker", "()Lnet/minecraft/client/DeltaTracker;", &[])?.l()?; let v = ctx.env.call_method(&dt, "getGameTimeDeltaPartialTick", "(Z)F", &[JValue::Bool(0)])?.f()?; let _ = ctx.env.delete_local_ref(dt); Ok(v) })();
    match partial { Ok(v) if v.is_finite() => v.clamp(0.0, 1.0), _ => { jni_bridge::clear_pending_exception(ctx.env); 1.0 } }
}
fn player_iterable<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    match ctx.env.call_method(&ctx.level, "players", "()Ljava/util/List;", &[]) { Ok(v) => v.l(), Err(_) => { jni_bridge::clear_pending_exception(ctx.env); ctx.env.call_method(&ctx.level, "entitiesForRendering", "()Ljava/lang/Iterable;", &[])?.l() } }
}
fn collect_armor_items(env: &mut JNIEnv<'_>, entity: &JObject<'_>, items: &mut Vec<NameTagItem>) -> JniResult<()> {
    let slot_class = jni_bridge::load_class(env, "net.minecraft.world.entity.EquipmentSlot")?;
    for (slot_name, item_slot) in [("HEAD", NameTagItemSlot::Head), ("CHEST", NameTagItemSlot::Chest), ("LEGS", NameTagItemSlot::Legs), ("FEET", NameTagItemSlot::Feet)] {
        let slot = env.get_static_field(&slot_class, slot_name, "Lnet/minecraft/world/entity/EquipmentSlot;")?.l()?;
        let stack = env.call_method(entity, "getItemBySlot", "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;", &[JValue::Object(&slot)])?.l()?;
        if let Some(item) = stack_summary(env, &stack, item_slot)? { items.push(item); }
        let _ = env.delete_local_ref(stack); let _ = env.delete_local_ref(slot);
    }
    Ok(())
}
fn push_stack_from_method(env: &mut JNIEnv<'_>, entity: &JObject<'_>, slot: NameTagItemSlot, items: &mut Vec<NameTagItem>) -> JniResult<()> {
    let Some(method) = slot.hand_method() else { return Ok(()); };
    let stack = match env.call_method(entity, method, "()Lnet/minecraft/world/item/ItemStack;", &[]) { Ok(v) => v.l()?, Err(_) => { jni_bridge::clear_pending_exception(env); return Ok(()); } };
    if let Some(item) = stack_summary(env, &stack, slot)? { items.push(item); }
    let _ = env.delete_local_ref(stack); Ok(())
}
fn stack_summary(env: &mut JNIEnv<'_>, stack: &JObject<'_>, slot: NameTagItemSlot) -> JniResult<Option<NameTagItem>> {
    if stack.is_null() || env.call_method(stack, "isEmpty", "()Z", &[])?.z()? { return Ok(None); }
    Ok(Some(NameTagItem { slot }))
}


pub fn build_world_geometry(
    env: &mut JNIEnv<'_>,
    lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) -> JniResult<()> {
    let Some(config) = WORLD_CONFIG.lock().ok().and_then(|s| s.clone()) else { return Ok(()); };
    let Some(mut ctx) = MinecraftContext::new(env)? else { return Ok(()); };
    if !minecraft::has_player_and_world(&ctx) { return Ok(()); }
    let player_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let player_team = team_name(ctx.env, &ctx.player).unwrap_or(None);
    let partial = frame_partial_ticks(&mut ctx);
    let player_pos = entity_position(ctx.env, &ctx.player, partial)?;
    let iterable = player_iterable(&mut ctx)?;
    let iter = ctx.env.call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?.l()?;
    while ctx.env.call_method(&iter, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx.env.call_method(&iter, "next", "()Ljava/lang/Object;", &[])?.l()?;
        if ctx.env.is_instance_of(&entity, &player_class).unwrap_or(false) {
            let _ = push_entity_tag(&mut ctx, &entity, &config, player_pos, &player_team, partial, lines, filled, camera);
            jni_bridge::clear_pending_exception(ctx.env);
        }
        let _ = ctx.env.delete_local_ref(entity);
    }
    let _ = ctx.env.delete_local_ref(iter); let _ = ctx.env.delete_local_ref(iterable);
    Ok(())
}

fn push_entity_tag(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    config: &NameTagsConfig,
    player_pos: Vec3d,
    player_team: &Option<String>,
    partial: f32,
    _lines: &mut crate::avatar::render::mesh::Mesh,
    filled: &mut crate::avatar::render::mesh::Mesh,
    camera: Vec3d,
) -> JniResult<()> {
    let is_self = ctx.env.is_same_object(entity, &ctx.player)?;
    if is_self && !config.show_self { return Ok(()); }
    if !minecraft::entity_is_alive(ctx, entity)? { return Ok(()); }
    if let Some(uuid) = entity_uuid(ctx, entity) { if crate::world_render::skip_player(uuid) { return Ok(()); } }
    let entity_pos = entity_position(ctx.env, entity, partial)?;
    let distance = player_pos.distance_to(entity_pos) as f32;
    if distance > config.range { return Ok(()); }
    if distance < 9.0 { return Ok(()); } 
    let entity_team = team_name(ctx.env, entity).unwrap_or(None);
    let same_team = player_team.is_some() && player_team == &entity_team;
    let scoreboard_name = minecraft::entity_scoreboard_name(ctx, entity).unwrap_or_default();
    let explicit_friend = friends::is_friend(&scoreboard_name);
    if (same_team || explicit_friend) && !config.show_friends { return Ok(()); }

    
    let height = optional_float(ctx.env, entity, "getBbHeight", 1.8);
    let tag_world_pos = Vec3d { x: entity_pos.x, y: entity_pos.y + height as f64 + 0.65, z: entity_pos.z };

    let health = optional_float(ctx.env, entity, "getHealth", 20.0) + optional_float(ctx.env, entity, "getAbsorptionAmount", 0.0);
    let max_health = optional_float(ctx.env, entity, "getMaxHealth", 20.0).max(1.0) + optional_float(ctx.env, entity, "getAbsorptionAmount", 0.0);
    let ratio = (health / max_health).clamp(0.0, 1.0);
    let health_txt = health_text(config.health_mode, health, max_health);
    let _health_clr = health_color(ratio);

    let prefix = if config.show_donate { sanitize_name_tag_text(&team_prefix(ctx.env, entity).unwrap_or_default()) } else { String::new() };
    let name = match entity_name(ctx.env, entity) { Ok(n) => n, Err(_) => { jni_bridge::clear_pending_exception(ctx.env); "Player".to_string() } };

    
    
    let scale = tag_scale(config, distance);
    let font_size = (12.5 * scale).clamp(8.0, 24.0);
    let pad_x = 7.0 * scale;
    let gap = 5.0 * scale;
    
    let mut text_width_px = estimate_text_width(&name, font_size) + estimate_text_width(&health_txt, font_size);
    if !prefix.is_empty() { text_width_px += estimate_text_width(&format!("{} ", prefix), font_size); }
    text_width_px += gap * if prefix.is_empty() {1.0} else {2.0};
    let width_px = (text_width_px + pad_x * 2.0).max(42.0 * scale);
    
    let world_width = width_px as f64 * 0.0048;
    let world_height = (font_size + 9.0 * scale) as f64 * 0.0048;
    let bg_alpha = (config.alpha * 0.92).clamp(0.0, 1.0);
    
    let bg_color = [0.09, 0.09, 0.10, bg_alpha * 0.78];
    let border_color = [0.14, 0.14, 0.17, bg_alpha];

    
    
    if explicit_friend || same_team {
        let fc = config.friend_color;
        let friend_col = [fc[0], fc[1], fc[2], bg_alpha * 0.55];
        geometry::push_billboard_quad(filled, camera, tag_world_pos, world_width * 1.06, world_height * 1.14, friend_col);
    }
    geometry::push_billboard_quad(filled, camera, tag_world_pos, world_width, world_height, bg_color);

    
    
    
    
    let _ = border_color;
    let _ = prefix;
    let _ = name;
    let _ = health_txt;
    Ok(())
}

fn entity_uuid(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> Option<uuid::Uuid> {
    let uuid = ctx.env.call_method(entity, "getUUID", "()Ljava/util/UUID;", &[]).ok()?.l().ok()?;
    let txt = ctx.env.call_method(&uuid, "toString", "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
    let s = ctx.env.get_string((&txt).into()).ok()?;
    uuid::Uuid::parse_str(&s.to_string_lossy()).ok()
}
