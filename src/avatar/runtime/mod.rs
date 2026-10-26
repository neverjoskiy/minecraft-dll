use std::{
    collections::HashMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use uuid::Uuid;
use zip::ZipArchive;

use crate::avatar::format::{
    AvatarPackage, AvatarSource, BbElement, BbModel, BbVec3, discover_package, load_package,
};
use crate::avatar::lua::{AvatarLuaRuntime, state::ScriptWorld};
use crate::jni_bridge;

pub const DEMO_CUBE_ID: &str = "xz:demo_cube";


static AVATAR_CONTENT_GEN: AtomicU64 = AtomicU64::new(1);

pub fn avatar_content_generation() -> u64 {
    AVATAR_CONTENT_GEN.load(Ordering::Relaxed)
}

fn bump_avatar_content_generation() {
    AVATAR_CONTENT_GEN.fetch_add(1, Ordering::Relaxed);
    crate::avatar::render::textures::clear_texture_cache();
    crate::avatar::render::clear_render_caches();
}

#[derive(Clone, Debug)]
pub struct LoadedAvatar {
    pub id: String,
    pub display_name: String,
    pub package: AvatarPackage,
    pub local_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerAvatarState {
    pub player: Uuid,
    pub active_id: Option<String>,
    pub status: String,
}

#[derive(Default)]
struct RegistryState {
    avatars: HashMap<String, LoadedAvatar>,
    players: HashMap<Uuid, PlayerAvatarState>,
    lua_runtimes: HashMap<Uuid, Arc<Mutex<AvatarLuaRuntime>>>,
    
    script_worlds: HashMap<String, crate::avatar::lua::state::SharedWorld>,
    
    preferred_local_id: Option<String>,
    status: String,
}

fn state() -> &'static Mutex<RegistryState> {
    static STATE: OnceLock<Mutex<RegistryState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let mut registry = RegistryState::default();
        registry.status = "ready".to_string();
        Mutex::new(registry)
    })
}

pub fn status() -> String {
    state()
        .lock()
        .map(|guard| guard.status.clone())
        .unwrap_or_else(|_| "registry poisoned".to_string())
}

pub fn set_status(message: impl Into<String>) {
    if let Ok(mut guard) = state().lock() {
        guard.status = message.into();
    }
}

pub fn list_avatar_ids() -> Vec<String> {
    state()
        .lock()
        .map(|guard| {
            let mut ids: Vec<_> = guard.avatars.keys().cloned().collect();
            ids.sort();
            ids
        })
        .unwrap_or_default()
}

pub fn list_avatars() -> Vec<LoadedAvatar> {
    state()
        .lock()
        .map(|guard| {
            let mut avatars: Vec<_> = guard.avatars.values().cloned().collect();
            avatars.sort_by(|a, b| a.display_name.cmp(&b.display_name));
            avatars
        })
        .unwrap_or_default()
}

pub fn preferred_local_avatar_id() -> String {
    state()
        .lock()
        .ok()
        .and_then(|guard| guard.preferred_local_id.clone())
        .unwrap_or_else(|| DEMO_CUBE_ID.to_string())
}

pub fn player_has_explicit_avatar(player: Uuid) -> bool {
    state()
        .lock()
        .ok()
        .map(|guard| {
            guard
                .players
                .get(&player)
                .and_then(|entry| entry.active_id.as_ref())
                .is_some()
                || guard.preferred_local_id.is_some()
        })
        .unwrap_or(false)
}

pub fn set_preferred_local_avatar(avatar_id: &str) -> Result<(), String> {
    let mut guard = state()
        .lock()
        .map_err(|_| "registry poisoned".to_string())?;
    if !guard.avatars.contains_key(avatar_id) {
        return Err(format!("unknown avatar: {avatar_id}"));
    }
    guard.preferred_local_id = Some(avatar_id.to_string());
    guard.status = format!("preferred local avatar {avatar_id}");
    Ok(())
}








pub fn use_vanilla_skin() -> Result<(), String> {
    {
        let mut guard = state()
            .lock()
            .map_err(|_| "registry poisoned".to_string())?;
        guard.preferred_local_id = None;
        for player_state in guard.players.values_mut() {
            player_state.active_id = None;
            player_state.status = "vanilla skin".to_string();
        }
        guard.lua_runtimes.clear();
        guard.status = "vanilla skin".to_string();
    }
    crate::avatar::render::set_replacement_enabled(false);
    bump_avatar_content_generation();
    Ok(())
}


pub fn vanilla_skin_active() -> bool {
    !crate::avatar::render::replacement_enabled()
}

pub fn get_avatar(id: &str) -> Option<LoadedAvatar> {
    state()
        .lock()
        .ok()
        .and_then(|guard| guard.avatars.get(id).cloned())
}


pub fn with_avatar<R>(id: &str, f: impl FnOnce(&LoadedAvatar) -> R) -> Option<R> {
    let guard = state().lock().ok()?;
    let avatar = guard.avatars.get(id)?;
    Some(f(avatar))
}

pub fn demo_cube() -> LoadedAvatar {
    demo_cube_avatar()
}


pub fn avatar_png(id: &str) -> Option<Vec<u8>> {
    with_avatar(id, |avatar| avatar.package.avatar_png.clone()).flatten()
}

pub fn load_local_path(path: &Path) -> io::Result<String> {
    let source = discover_package(path)?;
    let package = load_package(&source)?;
    let id = avatar_id_from_package(&package);
    let display_name = package.json.name.clone().unwrap_or_else(|| id.clone());

    let loaded = LoadedAvatar {
        id: id.clone(),
        display_name,
        package,
        local_path: Some(path.to_path_buf()),
    };

    if let Ok(mut guard) = state().lock() {
        guard.avatars.insert(id.clone(), loaded);
        guard.status = format!("loaded {id}");
    }
    bump_avatar_content_generation();

    Ok(id)
}

pub fn activate(player: Uuid, avatar_id: &str) -> Result<(), String> {
    let mut guard = state()
        .lock()
        .map_err(|_| "registry poisoned".to_string())?;
    let Some(avatar) = guard.avatars.get(avatar_id).cloned() else {
        return Err(format!("unknown avatar: {avatar_id}"));
    };

    guard.preferred_local_id = Some(avatar_id.to_string());
    let world = ScriptWorld::from_package(&avatar.package).share();
    guard
        .script_worlds
        .insert(avatar_id.to_string(), std::sync::Arc::clone(&world));
    let lua_result = spawn_lua_runtime(&avatar.package, std::sync::Arc::clone(&world));
    guard.status = format!("activated {avatar_id} for {player}");

    let player_status = match lua_result {
        Ok(runtime) => {
            let lua_status = runtime.status().to_string();
            guard
                .lua_runtimes
                .insert(player, Arc::new(Mutex::new(runtime)));
            format!("active: {avatar_id} ({lua_status})")
        }
        Err(message) => {
            guard.lua_runtimes.remove(&player);
            format!("active: {avatar_id} (lua: {message})")
        }
    };

    let entry = guard
        .players
        .entry(player)
        .or_insert_with(|| PlayerAvatarState {
            player,
            ..Default::default()
        });
    entry.active_id = Some(avatar_id.to_string());
    entry.status = player_status;
    drop(guard);

    
    crate::avatar::render::set_replacement_enabled(true);
    Ok(())
}


pub fn player_has_lua(player: Uuid) -> bool {
    state()
        .lock()
        .map(|guard| guard.lua_runtimes.contains_key(&player))
        .unwrap_or(false)
}

pub fn fire_lua_event(player: Uuid, event: &str) -> Result<(), String> {
    let runtime = state()
        .lock()
        .map_err(|_| "registry poisoned".to_string())?
        .lua_runtimes
        .get(&player)
        .cloned();
    let Some(runtime) = runtime else {
        return Ok(());
    };
    let runtime = runtime
        .lock()
        .map_err(|_| "lua runtime poisoned".to_string())?;
    runtime.fire_event(event, mlua::MultiValue::new())
}


fn spawn_lua_runtime(
    package: &AvatarPackage,
    world: crate::avatar::lua::state::SharedWorld,
) -> Result<AvatarLuaRuntime, String> {
    let scripts = collect_scripts(package).map_err(|err| err.to_string())?;
    if scripts.is_empty() {
        return Err("no scripts".to_string());
    }
    let modules = collect_all_lua_sources(package).unwrap_or_default();

    let mut runtime = AvatarLuaRuntime::new(world)?;
    runtime.install_require(&modules)?;
    runtime.load_scripts(&scripts)?;
    let _ = runtime.fire_event("ENTITY_INIT", mlua::MultiValue::new());
    Ok(runtime)
}


pub fn script_model_tree(avatar_id: &str) -> Option<crate::avatar::model::ModelPart> {
    let guard = state().lock().ok()?;
    let world = guard.script_worlds.get(avatar_id)?;
    let world = world.lock().ok()?;
    Some(world.models.clone())
}



pub fn overlay_script_transforms(avatar_id: &str, dest: &mut crate::avatar::model::ModelPart) {
    let world = state()
        .lock()
        .ok()
        .and_then(|guard| guard.script_worlds.get(avatar_id).cloned());
    let Some(world) = world else {
        return;
    };
    let Ok(world) = world.lock() else {
        return;
    };
    let script = if world.models.children.len() == 1 {
        &world.models.children[0]
    } else if !world.models.children.is_empty() {
        &world.models
    } else {
        return;
    };
    
    if script.name == dest.name || script.children.iter().any(|c| c.name == dest.name) {
        if script.name == dest.name {
            overlay_part_transforms(script, dest);
        } else if let Some(child) = script.children.iter().find(|c| c.name == dest.name) {
            overlay_part_transforms(child, dest);
        }
    } else {
        overlay_part_transforms(script, dest);
    }
}

fn overlay_part_transforms(
    script: &crate::avatar::model::ModelPart,
    dest: &mut crate::avatar::model::ModelPart,
) {
    dest.position = script.position;
    dest.rotation = script.rotation;
    dest.scale = script.scale;
    dest.visible = script.visible;
    let same_order = script.children.len() == dest.children.len()
        && script
            .children
            .iter()
            .zip(dest.children.iter())
            .all(|(s, d)| s.name == d.name);
    if same_order {
        for (s, d) in script.children.iter().zip(dest.children.iter_mut()) {
            overlay_part_transforms(s, d);
        }
    } else {
        for s in &script.children {
            if let Some(d) = dest.children.iter_mut().find(|c| c.name == s.name) {
                overlay_part_transforms(s, d);
            }
        }
    }
}





pub fn apply_playing_animations(
    avatar_id: &str,
    tree: &mut crate::avatar::model::ModelPart,
    dt: f32,
) {
    use crate::avatar::animation::{PlayState, tick_animation};

    let world = state()
        .lock()
        .ok()
        .and_then(|guard| guard.script_worlds.get(avatar_id).cloned());
    let Some(world) = world else {
        return;
    };
    let Ok(mut world) = world.lock() else {
        return;
    };

    
    let mut limit = 2048i32;
    for map in world.animations.values_mut() {
        for anim in map.values_mut() {
            if anim.play_state == PlayState::Stopped {
                continue;
            }
            if dt > 0.0 {
                anim.tick(dt);
            }
            
            if anim.play_state == PlayState::Stopped {
                continue;
            }
            limit = tick_animation(anim, tree, limit);
            if limit <= 0 {
                return;
            }
        }
    }
}



pub fn sync_movement_clips(
    avatar_id: &str,
    walk_speed: f32,
    sprinting: bool,
    crouching: bool,
    gliding: bool,
) {
    use crate::avatar::animation::PlayState;

    let world = state()
        .lock()
        .ok()
        .and_then(|guard| guard.script_worlds.get(avatar_id).cloned());
    let Some(world) = world else {
        return;
    };
    let Ok(mut world) = world.lock() else {
        return;
    };

    let preferred: &[&str] = if gliding {
        &["glide", "elytra"]
    } else if crouching {
        &["crouch"]
    } else if walk_speed > 0.1 {
        if sprinting {
            &["sprint", "walk"]
        } else {
            &["walk"]
        }
    } else {
        &["idle"]
    };

    const MOVEMENT: &[&str] = &[
        "idle",
        "walk",
        "walkback",
        "sprint",
        "crouch",
        "crouchwalk",
        "elytra",
        "glide",
    ];

    for map in world.animations.values_mut() {
        let chosen = preferred
            .iter()
            .find(|name| map.contains_key(**name))
            .copied();
        let Some(chosen) = chosen else {
            continue;
        };
        for (name, anim) in map.iter_mut() {
            if !MOVEMENT.iter().any(|m| name.eq_ignore_ascii_case(m)) {
                continue;
            }
            if name == chosen {
                if anim.play_state == PlayState::Stopped {
                    anim.play();
                }
            } else if anim.play_state != PlayState::Stopped {
                anim.stop();
            }
        }
    }
}

fn collect_scripts(package: &AvatarPackage) -> io::Result<Vec<(String, String)>> {
    let mut names = Vec::new();
    if let Some(auto) = &package.json.auto_scripts {
        names.extend(auto.iter().cloned());
    }
    if let Some(manual) = &package.json.scripts {
        for name in manual {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.clone());
            }
        }
    }

    
    
    if names.is_empty() {
        let discovered = discover_lua_names(package)?;
        if discovered
            .iter()
            .any(|n| n.eq_ignore_ascii_case("script.lua"))
        {
            names.push("script.lua".to_string());
        } else {
            names = discovered;
        }
    }

    if names.is_empty() {
        return Ok(Vec::new());
    }

    match &package.source {
        AvatarSource::Directory(root) => collect_scripts_from_dir(root, &names),
        AvatarSource::Zip(path) => collect_scripts_from_zip(path, &names),
    }
}

fn collect_all_lua_sources(package: &AvatarPackage) -> io::Result<Vec<(String, String)>> {
    let names = discover_lua_names(package)?;
    match &package.source {
        AvatarSource::Directory(root) => collect_scripts_from_dir(root, &names),
        AvatarSource::Zip(path) => collect_scripts_from_zip(path, &names),
    }
}

fn discover_lua_names(package: &AvatarPackage) -> io::Result<Vec<String>> {
    match &package.source {
        AvatarSource::Directory(root) => {
            let mut names = Vec::new();
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("lua") {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    names.push(name.to_string());
                }
            }
            names.sort();
            Ok(names)
        }
        AvatarSource::Zip(path) => {
            let file = fs::File::open(path)?;
            let mut archive = ZipArchive::new(file)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let mut names = Vec::new();
            for i in 0..archive.len() {
                let Ok(entry) = archive.by_index(i) else {
                    continue;
                };
                let name = entry.name().replace('\\', "/");
                if name.ends_with(".lua") && !name.contains('/') && !name.contains("..") {
                    names.push(name);
                }
            }
            names.sort();
            Ok(names)
        }
    }
}

fn collect_scripts_from_dir(root: &Path, names: &[String]) -> io::Result<Vec<(String, String)>> {
    let mut scripts = Vec::new();
    for name in names {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        scripts.push((name.clone(), source));
    }
    Ok(scripts)
}

fn collect_scripts_from_zip(path: &Path, names: &[String]) -> io::Result<Vec<(String, String)>> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

    let mut scripts = Vec::new();
    for name in names {
        let zip_path = name.replace('\\', "/");
        if let Ok(mut entry) = archive.by_name(&zip_path) {
            let mut source = String::new();
            if entry.read_to_string(&mut source).is_ok() {
                scripts.push((name.clone(), source));
            }
        }
    }
    Ok(scripts)
}

pub fn unload(avatar_id: &str) -> Result<(), String> {
    if avatar_id == DEMO_CUBE_ID {
        return Err("cannot unload built-in demo avatar".into());
    }

    let mut guard = state()
        .lock()
        .map_err(|_| "registry poisoned".to_string())?;
    if guard.avatars.remove(avatar_id).is_none() {
        return Err(format!("unknown avatar: {avatar_id}"));
    }

    let affected_players: Vec<_> = guard
        .players
        .iter()
        .filter_map(|(player, player_state)| {
            (player_state.active_id.as_deref() == Some(avatar_id)).then_some(*player)
        })
        .collect();
    for player in affected_players {
        guard.lua_runtimes.remove(&player);
        if let Some(player_state) = guard.players.get_mut(&player) {
            player_state.active_id = Some(DEMO_CUBE_ID.to_string());
            player_state.status = "fallback to demo cube".to_string();
        }
    }
    guard.script_worlds.remove(avatar_id);
    if guard.preferred_local_id.as_deref() == Some(avatar_id) {
        guard.preferred_local_id = None;
    }

    guard.status = format!("unloaded {avatar_id}");
    drop(guard);
    bump_avatar_content_generation();
    Ok(())
}

pub fn hot_reload(avatar_id: &str) -> io::Result<()> {
    let path = {
        let guard = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "registry poisoned"))?;
        guard
            .avatars
            .get(avatar_id)
            .and_then(|avatar| avatar.local_path.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "avatar has no local path"))?
    };

    let new_id = load_local_path(&path)?;
    if new_id != avatar_id {
        let mut guard = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "registry poisoned"))?;
        if let Some(avatar) = guard.avatars.remove(&new_id) {
            guard.avatars.insert(avatar_id.to_string(), avatar);
        }
        guard.avatars.remove(&new_id);
    }

    let players = {
        let guard = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "registry poisoned"))?;
        guard
            .players
            .iter()
            .filter_map(|(player, player_state)| {
                (player_state.active_id.as_deref() == Some(avatar_id)).then_some(*player)
            })
            .collect::<Vec<_>>()
    };
    for player in players {
        activate(player, avatar_id).map_err(io::Error::other)?;
    }

    if let Ok(mut guard) = state().lock() {
        guard.status = format!("hot reloaded {avatar_id}");
    }
    Ok(())
}

pub fn player_state(player: Uuid) -> Option<PlayerAvatarState> {
    state()
        .lock()
        .ok()
        .and_then(|guard| guard.players.get(&player).cloned())
}

pub fn active_avatar(player: Uuid) -> Option<LoadedAvatar> {
    let id = active_avatar_id(player)?;
    get_avatar(&id)
}


pub fn active_avatar_id(player: Uuid) -> Option<String> {
    state().lock().ok().and_then(|guard| {
        guard
            .players
            .get(&player)
            .and_then(|state| state.active_id.clone())
            .or_else(|| guard.preferred_local_id.clone())
            .or_else(|| Some(DEMO_CUBE_ID.to_string()))
    })
}


pub fn activate_local(avatar_id: &str) -> Result<Uuid, String> {
    let player = local_player_uuid().unwrap_or(Uuid::nil());
    activate(player, avatar_id)?;
    if player != Uuid::nil() {
        
        let mut guard = state()
            .lock()
            .map_err(|_| "registry poisoned".to_string())?;
        let entry = guard
            .players
            .entry(Uuid::nil())
            .or_insert_with(|| PlayerAvatarState {
                player: Uuid::nil(),
                ..Default::default()
            });
        entry.active_id = Some(avatar_id.to_string());
        entry.status = format!("mirror of {player}");
    }
    if !crate::avatar::render::replacement_enabled() {
        let detail = crate::native_jvmti::status::snapshot().last_error;
        return Err(if detail.is_empty() {
            "avatar activated but render hooks failed to arm".into()
        } else {
            format!("avatar activated but hooks failed: {detail}")
        });
    }
    Ok(player)
}

fn local_player_uuid() -> Option<Uuid> {
    jni_bridge::with_env(|env| {
        let class = jni_bridge::load_class(env, "net.minecraft.client.Minecraft")?;
        let instance = env
            .call_static_method(
                class,
                "getInstance",
                "()Lnet/minecraft/client/Minecraft;",
                &[],
            )?
            .l()?;
        let local = env
            .get_field(
                instance,
                "player",
                "Lnet/minecraft/client/player/LocalPlayer;",
            )?
            .l()?;
        if local.is_null() {
            return Err(jni::errors::Error::NullPtr("local player"));
        }
        let uuid = env
            .call_method(local, "getUUID", "()Ljava/util/UUID;", &[])?
            .l()?;
        let text = env
            .call_method(uuid, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        let java_str = env.get_string((&text).into())?;
        Uuid::parse_str(&java_str.to_string_lossy())
            .map_err(|_| jni::errors::Error::NullPtr("uuid parse"))
    })
}

pub fn with_active_avatar<R>(player: Uuid, f: impl FnOnce(&LoadedAvatar) -> R) -> Option<R> {
    active_avatar(player).map(|avatar| f(&avatar))
}

#[derive(Clone, Debug, Default)]
pub struct RenderAcceptanceSnapshot {
    pub avatar_id: String,
    pub element_count: usize,
    pub status: String,
}

pub fn render_snapshot(player: Uuid) -> RenderAcceptanceSnapshot {
    active_avatar_id(player)
        .and_then(|id| {
            with_avatar(&id, |avatar| RenderAcceptanceSnapshot {
                avatar_id: avatar.id.clone(),
                element_count: avatar
                    .package
                    .models
                    .iter()
                    .map(|model| model.elements.len())
                    .sum(),
                status: "snapshot ok".to_string(),
            })
        })
        .unwrap_or_else(|| RenderAcceptanceSnapshot {
            avatar_id: DEMO_CUBE_ID.to_string(),
            element_count: 1,
            status: "demo fallback".to_string(),
        })
}

fn avatar_id_from_package(package: &AvatarPackage) -> String {
    if let Some(name) = &package.json.name {
        let slug = sanitize_id(name);
        if !slug.is_empty() {
            return slug;
        }
    }
    match &package.source {
        AvatarSource::Directory(path) | AvatarSource::Zip(path) => sanitize_id(
            &path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("avatar")
                .to_string(),
        ),
    }
}

fn sanitize_id(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    out.chars().take(48).collect()
}

fn demo_cube_avatar() -> LoadedAvatar {
    let cube = BbElement {
        name: "demo_cube".to_string(),
        from: BbVec3 {
            x: -4.0,
            y: 0.0,
            z: -4.0,
        },
        to: BbVec3 {
            x: 4.0,
            y: 8.0,
            z: 4.0,
        },
        ..Default::default()
    };

    let model = BbModel {
        name: "demo".to_string(),
        elements: vec![cube],
        ..Default::default()
    };

    LoadedAvatar {
        id: DEMO_CUBE_ID.to_string(),
        display_name: "Demo Cube".to_string(),
        package: AvatarPackage {
            source: AvatarSource::Directory(PathBuf::from(DEMO_CUBE_ID)),
            json: crate::avatar::format::AvatarJson {
                name: Some("Demo Cube".to_string()),
                authors: Some(vec!["XZ".to_string()]),
                version: Some("0.0.0".to_string()),
                ..Default::default()
            },
            root: PathBuf::from(DEMO_CUBE_ID),
            models: vec![model],
            metadata_nbt: None,
            avatar_png: None,
        },
        local_path: None,
    }
}
