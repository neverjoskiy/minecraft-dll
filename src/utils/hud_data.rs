use std::sync::{Mutex, OnceLock};

use jni::{
    errors::Result as JniResult,
    objects::{JObject, JString, JValue},
};

use crate::{
    jni_bridge,
    utils::{
        aim_target,
        minecraft::{self, MinecraftContext},
        name_tags::NameTagItemSlot,
    },
};

#[derive(Clone, Debug, Default)]
pub struct HudInfoData {
    pub user: String,
    pub ping_ms: i32,
}

#[derive(Clone, Debug)]
pub struct HudTargetData {
    pub entity_id: i32,
    pub name: String,
    pub health: f32,
    pub max_health: f32,
    pub armor: [bool; 4], 
}

#[derive(Clone, Debug)]
pub struct HudStaffEntry {
    pub name: String,
    pub role: String,
    pub online: bool,
    pub spectating: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HudWorldData {
    pub info: HudInfoData,
    pub local_entity_id: Option<i32>,
    pub target: Option<HudTargetData>,
    pub local_armor: [bool; 4],
    pub staff: Vec<HudStaffEntry>,
}

static CACHED: OnceLock<Mutex<HudWorldData>> = OnceLock::new();

fn cache() -> &'static Mutex<HudWorldData> {
    CACHED.get_or_init(|| Mutex::new(HudWorldData::default()))
}

pub fn cached() -> HudWorldData {
    cache()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

pub fn refresh() {
    let data = jni_bridge::with_env(|env| {
        let result = collect(env);
        if result.is_err() {
            jni_bridge::clear_pending_exception(env);
        }
        result
    })
    .unwrap_or_default();

    if let Ok(mut guard) = cache().lock() {
        *guard = data;
    }
}

pub fn clear() {
    if let Ok(mut guard) = cache().lock() {
        *guard = HudWorldData::default();
    }
}

pub fn pc_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown".to_string())
}

fn collect(env: &mut jni::JNIEnv<'_>) -> JniResult<HudWorldData> {
    let mut data = HudWorldData {
        info: HudInfoData {
            user: pc_username(),
            ping_ms: 0,
        },
        ..HudWorldData::default()
    };

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(data);
    };
    if !minecraft::has_player_and_world(&ctx) {
        return Ok(data);
    }

    data.local_entity_id = Some(ctx.env.call_method(&ctx.player, "getId", "()I", &[])?.i()?);
    data.info.ping_ms = match local_ping(&mut ctx) {
        Ok(v) => v,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            0
        }
    };
    data.local_armor = match armor_flags(ctx.env, &ctx.player) {
        Ok(v) => v,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            [false; 4]
        }
    };
    data.target = match resolve_target(&mut ctx) {
        Ok(v) => v,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            None
        }
    };
    data.staff = match collect_staff(&mut ctx) {
        Ok(v) => v,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            Vec::new()
        }
    };
    Ok(data)
}

fn resolve_target(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Option<HudTargetData>> {
    let overlay = aim_target::overlay_state();
    let entity_id = if overlay.target_active {
        overlay.target_entity_id
    } else {
        None
    }
    .or_else(|| minecraft::vanilla_crosshair_entity_id(ctx).ok().flatten());

    let Some(entity_id) = entity_id else {
        return Ok(None);
    };
    let Some(entity) = ctx.entity_by_id(entity_id)? else {
        return Ok(None);
    };

    let living = is_living(ctx.env, &entity).unwrap_or(false);
    if !living {
        let _ = ctx.env.delete_local_ref(entity);
        return Ok(None);
    }

    let name = entity_display_name(ctx.env, &entity).unwrap_or_else(|_| format!("#{entity_id}"));
    let health = call_float(ctx.env, &entity, "getHealth", 0.0)
        + call_float(ctx.env, &entity, "getAbsorptionAmount", 0.0);
    let max_health = call_float(ctx.env, &entity, "getMaxHealth", 20.0).max(1.0)
        + call_float(ctx.env, &entity, "getAbsorptionAmount", 0.0);
    let armor = armor_flags(ctx.env, &entity).unwrap_or([false; 4]);
    let _ = ctx.env.delete_local_ref(entity);

    Ok(Some(HudTargetData {
        entity_id,
        name,
        health,
        max_health,
        armor,
    }))
}

fn armor_flags<'local>(
    env: &mut jni::JNIEnv<'local>,
    entity: &JObject<'local>,
) -> JniResult<[bool; 4]> {
    let slots = [
        NameTagItemSlot::Head,
        NameTagItemSlot::Chest,
        NameTagItemSlot::Legs,
        NameTagItemSlot::Feet,
    ];
    let mut out = [false; 4];
    for (i, slot) in slots.into_iter().enumerate() {
        out[i] = slot_has_item(env, entity, slot)?;
    }
    Ok(out)
}

fn slot_has_item<'local>(
    env: &mut jni::JNIEnv<'local>,
    entity: &JObject<'local>,
    slot: NameTagItemSlot,
) -> JniResult<bool> {
    let stack = equipment_stack(env, entity, slot)?;
    if stack.is_null() {
        return Ok(false);
    }
    let empty = env.call_method(&stack, "isEmpty", "()Z", &[])?.z()?;
    let _ = env.delete_local_ref(stack);
    Ok(!empty)
}

fn equipment_stack<'local>(
    env: &mut jni::JNIEnv<'local>,
    entity: &JObject<'local>,
    slot: NameTagItemSlot,
) -> JniResult<JObject<'local>> {
    let name = match slot {
        NameTagItemSlot::Head => "HEAD",
        NameTagItemSlot::Chest => "CHEST",
        NameTagItemSlot::Legs => "LEGS",
        NameTagItemSlot::Feet => "FEET",
        NameTagItemSlot::MainHand | NameTagItemSlot::OffHand => "HEAD",
    };
    let slot_class = jni_bridge::load_class(env, "net.minecraft.world.entity.EquipmentSlot")?;
    let java_slot = env
        .get_static_field(
            &slot_class,
            name,
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

fn local_ping(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<i32> {
    let connection = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getConnection",
            "()Lnet/minecraft/client/multiplayer/ClientPacketListener;",
            &[],
        )?
        .l()?;
    if connection.is_null() {
        return Ok(0);
    }

    let uuid = ctx
        .env
        .call_method(&ctx.player, "getUUID", "()Ljava/util/UUID;", &[])?
        .l()?;

    let info = match ctx.env.call_method(
        &connection,
        "getPlayerInfo",
        "(Ljava/util/UUID;)Lnet/minecraft/client/multiplayer/PlayerInfo;",
        &[JValue::Object(&uuid)],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            
            match ctx.env.call_method(
                &connection,
                "getPlayerListEntry",
                "(Ljava/util/UUID;)Lnet/minecraft/client/network/PlayerListEntry;",
                &[JValue::Object(&uuid)],
            ) {
                Ok(value) => value.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(ctx.env);
                    let _ = ctx.env.delete_local_ref(uuid);
                    let _ = ctx.env.delete_local_ref(connection);
                    return Ok(0);
                }
            }
        }
    };

    let ping = if info.is_null() {
        0
    } else {
        ctx.env
            .call_method(&info, "getLatency", "()I", &[])
            .map(|v| v.i().unwrap_or(0))
            .unwrap_or(0)
    };
    if !info.is_null() {
        let _ = ctx.env.delete_local_ref(info);
    }
    let _ = ctx.env.delete_local_ref(uuid);
    let _ = ctx.env.delete_local_ref(connection);
    Ok(ping)
}

fn collect_staff(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec<HudStaffEntry>> {
    let connection = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getConnection",
            "()Lnet/minecraft/client/multiplayer/ClientPacketListener;",
            &[],
        )?
        .l()?;
    if connection.is_null() {
        return Ok(Vec::new());
    }

    let list = match ctx.env.call_method(
        &connection,
        "getOnlinePlayers",
        "()Ljava/util/Collection;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            match ctx.env.call_method(
                &connection,
                "getListedOnlinePlayers",
                "()Ljava/util/Collection;",
                &[],
            ) {
                Ok(value) => value.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(ctx.env);
                    match ctx.env.call_method(
                        &connection,
                        "getPlayerList",
                        "()Ljava/util/Collection;",
                        &[],
                    ) {
                        Ok(value) => value.l()?,
                        Err(_) => {
                            jni_bridge::clear_pending_exception(ctx.env);
                            let _ = ctx.env.delete_local_ref(connection);
                            return Ok(Vec::new());
                        }
                    }
                }
            }
        }
    };

    let iterator = ctx
        .env
        .call_method(&list, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;
    let mut staff = Vec::new();
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entry = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        if let Some(parsed) = parse_staff_entry(ctx.env, &entry)? {
            staff.push(parsed);
        }
        let _ = ctx.env.delete_local_ref(entry);
        if staff.len() >= 24 {
            break;
        }
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(list);
    let _ = ctx.env.delete_local_ref(connection);
    staff.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    Ok(staff)
}

fn parse_staff_entry(
    env: &mut jni::JNIEnv<'_>,
    entry: &JObject<'_>,
) -> JniResult<Option<HudStaffEntry>> {
    let profile = match env.call_method(
        entry,
        "getProfile",
        "()Lcom/mojang/authlib/GameProfile;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    let name_obj = match env.call_method(&profile, "name", "()Ljava/lang/String;", &[]) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            
            match env.call_method(&profile, "getName", "()Ljava/lang/String;", &[]) {
                Ok(value) => value.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(env);
                    let _ = env.delete_local_ref(profile);
                    return Ok(None);
                }
            }
        }
    };
    let name = jstring_to_string(env, name_obj.into())?;
    let _ = env.delete_local_ref(profile);
    if name.len() < 3
        || name.len() > 16
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Ok(None);
    }

    let team = match env.call_method(
        entry,
        "getTeam",
        "()Lnet/minecraft/world/scores/PlayerTeam;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            JObject::null()
        }
    };

    let prefix = if team.is_null() {
        String::new()
    } else {
        let prefix_comp = env
            .call_method(
                &team,
                "getPlayerPrefix",
                "()Lnet/minecraft/network/chat/Component;",
                &[],
            )
            .or_else(|_| {
                jni_bridge::clear_pending_exception(env);
                env.call_method(
                    &team,
                    "getPrefix",
                    "()Lnet/minecraft/network/chat/Component;",
                    &[],
                )
            });
        let text = match prefix_comp {
            Ok(value) => {
                let comp = value.l()?;
                let s = component_string(env, &comp).unwrap_or_default();
                let _ = env.delete_local_ref(comp);
                s
            }
            Err(_) => {
                jni_bridge::clear_pending_exception(env);
                String::new()
            }
        };
        let _ = env.delete_local_ref(team);
        text
    };

    let Some(role) = classify_staff_prefix(&prefix) else {
        return Ok(None);
    };

    let spectating = match env.call_method(
        entry,
        "getGameMode",
        "()Lnet/minecraft/world/level/GameType;",
        &[],
    ) {
        Ok(value) => {
            let mode = value.l()?;
            let name = env
                .call_method(&mode, "getName", "()Ljava/lang/String;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .and_then(|s| jstring_to_string(env, s.into()).ok())
                .unwrap_or_default();
            let _ = env.delete_local_ref(mode);
            name.eq_ignore_ascii_case("spectator")
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            false
        }
    };

    Ok(Some(HudStaffEntry {
        name,
        role,
        online: !spectating,
        spectating,
    }))
}

fn classify_staff_prefix(prefix: &str) -> Option<String> {
    let s = strip_formatting(prefix).to_lowercase();
    if s.is_empty() {
        return None;
    }
    let keywords = [
        ("owner", "Owner"),
        ("admin", "Admin"),
        ("админ", "Admin"),
        ("moder", "Moder"),
        ("модер", "Moder"),
        ("helper", "Helper"),
        ("хелпер", "Helper"),
        ("curator", "Curator"),
        ("куратор", "Curator"),
        ("поддержка", "Support"),
        ("сотрудник", "Staff"),
        ("зам", "Deputy"),
        ("стажёр", "Trainee"),
        ("стажер", "Trainee"),
        ("staff", "Staff"),
        ("yt", "Media"),
        ("youtube", "Media"),
    ];
    for (needle, role) in keywords {
        if s.contains(needle) {
            return Some(role.to_string());
        }
    }
    None
}

fn strip_formatting(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            let _ = chars.next();
            continue;
        }
        out.push(ch);
    }
    out
}

fn is_living(env: &mut jni::JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<bool> {
    let class = jni_bridge::load_class(env, "net.minecraft.world.entity.LivingEntity")?;
    Ok(env.is_instance_of(entity, &class)?)
}

fn entity_display_name(env: &mut jni::JNIEnv<'_>, entity: &JObject<'_>) -> JniResult<String> {
    if let Ok(name) = env.call_method(entity, "getScoreboardName", "()Ljava/lang/String;", &[]) {
        let obj = name.l()?;
        if let Ok(s) = jstring_to_string(env, obj.into()) {
            if !s.is_empty() {
                return Ok(s);
            }
        }
    } else {
        jni_bridge::clear_pending_exception(env);
    }

    let display = env
        .call_method(
            entity,
            "getName",
            "()Lnet/minecraft/network/chat/Component;",
            &[],
        )?
        .l()?;
    let text = component_string(env, &display).unwrap_or_default();
    let _ = env.delete_local_ref(display);
    Ok(text)
}

fn component_string(env: &mut jni::JNIEnv<'_>, component: &JObject<'_>) -> JniResult<String> {
    let s = env
        .call_method(component, "getString", "()Ljava/lang/String;", &[])?
        .l()?;
    jstring_to_string(env, s.into())
}

fn jstring_to_string(env: &mut jni::JNIEnv<'_>, value: JString<'_>) -> JniResult<String> {
    let text = env.get_string(&value)?.to_string_lossy().into_owned();
    let _ = env.delete_local_ref(value);
    Ok(text)
}

fn call_float(env: &mut jni::JNIEnv<'_>, obj: &JObject<'_>, method: &str, default: f32) -> f32 {
    match env.call_method(obj, method, "()F", &[]) {
        Ok(value) => value.f().unwrap_or(default),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            default
        }
    }
}
