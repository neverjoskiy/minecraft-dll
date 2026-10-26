use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use jni::{
    JNIEnv,
    objects::{GlobalRef, JClass, JFieldID, JObject, JValue},
    signature::{Primitive, ReturnType},
    sys::{jboolean, jobject},
};
use uuid::Uuid;

use crate::{
    avatar::{
        render::{
            contexts::{LayerPolicy, RenderContext, layer_policy},
            mesh::{self, Mesh},
            textures,
        },
        runtime::{self, DEMO_CUBE_ID},
    },
    jni_bridge,
    native_jvmti::{self, HookSpec, LocalLoad, RENDER_SYMBOLS},
};

static REPLACE_ENABLED: AtomicBool = AtomicBool::new(false);
static HOOKS_ARMED: AtomicBool = AtomicBool::new(false);



static AVATAR_HOOKS_REGISTERED: AtomicBool = AtomicBool::new(false);
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static DISPATCHER_HITS: AtomicU64 = AtomicU64::new(0);
static DISPATCHER_REPLACED: AtomicU64 = AtomicU64::new(0);
static LIVING_HITS: AtomicU64 = AtomicU64::new(0);
static LIVING_REPLACED: AtomicU64 = AtomicU64::new(0);
static BODY_VISIBLE_SKIP: AtomicU64 = AtomicU64::new(0);
static EMIT_PACKED_OK: AtomicU64 = AtomicU64::new(0);
static EMIT_PACKED_FAIL: AtomicU64 = AtomicU64::new(0);
static MESH_EMIT_READY: AtomicBool = AtomicBool::new(false);
static MESH_CACHE: OnceLock<Mutex<HashMap<u64, Mesh>>> = OnceLock::new();
static MESH_GENERATIONS: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();
static AVATAR_RENDER_STATE_CLASS: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();
static LOCAL_PLAYER_IDENTITY: OnceLock<Mutex<Option<(i32, Uuid)>>> = OnceLock::new();
static GEOMETRY_RENDERER: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();

static HANDLE_RENDERERS: OnceLock<Mutex<HashMap<i64, GlobalRef>>> = OnceLock::new();

static RENDER_TYPE_CACHE: OnceLock<Mutex<HashMap<String, GlobalRef>>> = OnceLock::new();

static PACKED_ARRAY_CACHE: OnceLock<Mutex<HashMap<u64, PackedFloatArray>>> = OnceLock::new();

fn meshes() -> &'static Mutex<HashMap<u64, Mesh>> {
    MESH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mesh_generations() -> &'static Mutex<HashMap<u64, u64>> {
    MESH_GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn avatar_render_state_class() -> &'static Mutex<Option<GlobalRef>> {
    AVATAR_RENDER_STATE_CLASS.get_or_init(|| Mutex::new(None))
}

fn local_player_identity_cache() -> &'static Mutex<Option<(i32, Uuid)>> {
    LOCAL_PLAYER_IDENTITY.get_or_init(|| Mutex::new(None))
}

fn handle_renderers() -> &'static Mutex<HashMap<i64, GlobalRef>> {
    HANDLE_RENDERERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn render_type_cache() -> &'static Mutex<HashMap<String, GlobalRef>> {
    RENDER_TYPE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn packed_array_cache() -> &'static Mutex<HashMap<u64, PackedFloatArray>> {
    PACKED_ARRAY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn clear_render_caches() {
    if let Ok(mut guard) = meshes().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = render_type_cache().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = handle_renderers().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = packed_array_cache().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = mesh_generations().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = local_player_identity_cache().lock() {
        *guard = None;
    }
    if let Ok(mut slot) = draw_cache().lock() {
        *slot = None;
    }
}

pub fn set_replacement_enabled(enabled: bool) {
    REPLACE_ENABLED.store(enabled, Ordering::Release);
    if !enabled {
        return;
    }
    
    let in_world = jni_bridge::is_world_active();
    if !in_world {
        native_jvmti::status::set_error("join a world before Activate (JVMTI deferred)");
        REPLACE_ENABLED.store(false, Ordering::Release);
        return;
    }
    if let Err(error) = native_jvmti::install() {
        native_jvmti::status::set_error(format!("jvmti phase1: {error}"));
        REPLACE_ENABLED.store(false, Ordering::Release);
        return;
    }
    if let Err(error) = native_jvmti::install_trampoline() {
        native_jvmti::status::set_error(format!("trampoline: {error}"));
        REPLACE_ENABLED.store(false, Ordering::Release);
        return;
    }
    if let Err(error) = ensure_render_hooks_armed() {
        native_jvmti::status::set_error(format!("arm render hooks: {error}"));
        REPLACE_ENABLED.store(false, Ordering::Release);
    }
}

pub fn replacement_enabled() -> bool {
    REPLACE_ENABLED.load(Ordering::Acquire)
}


pub fn ensure_render_hooks_armed() -> Result<(), String> {
    if HOOKS_ARMED.load(Ordering::Acquire) {
        return Ok(());
    }
    install_render_hooks()?;
    HOOKS_ARMED.store(true, Ordering::Release);
    Ok(())
}




pub fn register_avatar_hooks_if_active() -> Result<(), String> {
    if !AVATAR_HOOKS_REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }
    register_hand_hooks()?;
    register_avatar_body_hooks(true)
}


#[cfg(test)]
pub fn readd_world_hooks_for_test() {
    readd_world_hooks();
}



fn readd_world_hooks() {
    if !crate::world_render::armed() {
        return;
    }
    #[cfg(test)]
    {
        native_jvmti::register_hook_spec_for_test(crate::world_render::world_hook_spec());
    }
    #[cfg(not(test))]
    {
        let _ = crate::world_render::register_world_hooks();
    }
}

pub fn install_render_hooks() -> Result<(), String> {
    
    
    native_jvmti::clear_hooks();
    readd_world_hooks();
    register_hand_hooks()?;
    native_jvmti::status::clear_error();
    let transformed_before = native_jvmti::status::snapshot().transformed_classes;
    let hand_classes = native_jvmti::arm_registered_hooks()
        .map_err(|error| format!("hand hooks failed: {error}"))?;
    if hand_classes == 0 {
        return Err("hand hooks retransformed 0 classes".into());
    }
    if native_jvmti::status::snapshot().transformed_classes == transformed_before {
        return Err("hand ClassFileLoadHook did not transform a class".into());
    }

    native_jvmti::clear_hooks();
    readd_world_hooks();
    register_hand_hooks()?;
    register_avatar_body_hooks(true)?;
    native_jvmti::status::clear_error();
    let transformed_before = native_jvmti::status::snapshot().transformed_classes;
    let mut body_result = native_jvmti::arm_registered_hooks();
    let mut body_err = native_jvmti::status::snapshot().last_error;
    let mut body_ok = body_result.is_ok()
        && native_jvmti::status::snapshot().transformed_classes > transformed_before
        && render_hook_status_ok(&body_err);
    let mut fast_body = body_ok;

    if !body_ok {
        native_jvmti::clear_hooks();
        readd_world_hooks();
        register_hand_hooks()?;
        register_avatar_body_hooks(false)?;
        native_jvmti::status::clear_error();
        let transformed_before = native_jvmti::status::snapshot().transformed_classes;
        body_result = native_jvmti::arm_registered_hooks();
        body_err = native_jvmti::status::snapshot().last_error;
        body_ok = body_result.is_ok()
            && native_jvmti::status::snapshot().transformed_classes > transformed_before
            && render_hook_status_ok(&body_err);
        fast_body = false;
    }

    if !body_ok {
        native_jvmti::clear_hooks();
        readd_world_hooks();
        register_hand_hooks()?;
        native_jvmti::status::clear_error();
        let _ = native_jvmti::arm_registered_hooks();
    }

    let targets = native_jvmti::hooked_class_names()
        .into_iter()
        .map(|name| name.replace('/', "."))
        .collect::<Vec<_>>()
        .join(",");
    crate::diag::log(format!(
        "avatar: hooks armed body_ok={body_ok} fast={fast_body} err={body_err:?} targets={targets}"
    ));
    native_jvmti::status::update(|status| {
        status.last_error = if body_ok {
            if fast_body {
                format!("ok fast-body targets={targets}")
            } else {
                format!("ok fallback-body targets={targets}")
            }
        } else {
            format!("hands ok; body unavailable ({body_err})")
        };
    });
    AVATAR_HOOKS_REGISTERED.store(body_ok, Ordering::Release);
    if body_ok {
        Ok(())
    } else {
        Err(format!("body hooks failed: {body_err}"))
    }
}

fn render_hook_status_ok(error: &str) -> bool {
    !error.contains("FAILS_VERIFICATION")
        && (error.is_empty()
            || error.starts_with("ok ")
            || error.starts_with("tp ")
            || error.starts_with("anim ")
            || error.starts_with("fp "))
}

fn register_hand_hooks() -> Result<(), String> {
    for (method, callback) in [
        (RENDER_SYMBOLS.render_right_hand, "onRightHand"),
        (RENDER_SYMBOLS.render_left_hand, "onLeftHand"),
    ] {
        native_jvmti::register_hook(HookSpec::gate(
            [RENDER_SYMBOLS.avatar_renderer],
            method.name,
            method.descriptor,
            callback,
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;ILjava/lang/Object;Z)Z",
            vec![
                LocalLoad::Reference(0),
                LocalLoad::Reference(1),
                LocalLoad::Reference(2),
                LocalLoad::Int(3),
                LocalLoad::Reference(4),
                LocalLoad::Int(5),
            ],
        ))?;
    }
    Ok(())
}

fn register_avatar_body_hooks(include_living_gate: bool) -> Result<(), String> {
    if include_living_gate {
        native_jvmti::register_hook(HookSpec::gate(
            [RENDER_SYMBOLS.living_entity_renderer],
            RENDER_SYMBOLS.living_submit.name,
            RENDER_SYMBOLS.living_submit.descriptor,
            "onLivingSubmit",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
            vec![
                LocalLoad::Reference(0),
                LocalLoad::Reference(1),
                LocalLoad::Reference(2),
                LocalLoad::Reference(3),
                LocalLoad::Reference(4),
            ],
        ))?;
    }

    native_jvmti::register_hook(HookSpec::replace(
        [RENDER_SYMBOLS.avatar_renderer],
        "scale",
        "(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;)V",
        "onAvatarScale",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        vec![
            LocalLoad::Reference(0),
            LocalLoad::Reference(1),
            LocalLoad::Reference(2),
        ],
    ))?;
    native_jvmti::register_hook(HookSpec::replace(
        [RENDER_SYMBOLS.avatar_renderer],
        "shouldRenderLayers",
        "(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;)Z",
        "onShouldRenderLayers",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        vec![LocalLoad::Reference(0), LocalLoad::Reference(1)],
    ))?;
    native_jvmti::register_hook(HookSpec::replace(
        [RENDER_SYMBOLS.living_entity_renderer],
        "isBodyVisible",
        "(Lnet/minecraft/client/renderer/entity/state/LivingEntityRenderState;)Z",
        "onIsBodyVisible",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        vec![LocalLoad::Reference(0), LocalLoad::Reference(1)],
    ))?;
    native_jvmti::register_hook(HookSpec::replace(
        [RENDER_SYMBOLS.avatar_renderer],
        "submitNameDisplay",
        "(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;Lnet/minecraft/client/renderer/state/level/CameraRenderState;)V",
        "onAvatarBody",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        vec![
            LocalLoad::Reference(0),
            LocalLoad::Reference(1),
            LocalLoad::Reference(2),
            LocalLoad::Reference(3),
            LocalLoad::Reference(4),
        ],
    ))?;
    Ok(())
}


pub fn on_avatar_scale(
    env: &mut JNIEnv<'_>,
    _renderer: &JObject<'_>,
    state: &JObject<'_>,
    pose_stack: &JObject<'_>,
) -> bool {
    if pose_stack.is_null() {
        return false;
    }
    let replace = should_replace_avatar_body(env, state);
    let s = if replace { 0.0 } else { 0.9375 };
    if env
        .call_method(
            pose_stack,
            "scale",
            "(FFF)V",
            &[JValue::Float(s), JValue::Float(s), JValue::Float(s)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        return false;
    }
    replace
}


pub fn on_should_render_layers(
    env: &mut JNIEnv<'_>,
    _renderer: &JObject<'_>,
    state: &JObject<'_>,
) -> bool {
    if should_replace_avatar_body(env, state) {
        return false;
    }
    !read_bool_field(env, state, &["isSpectator"])
}



pub fn on_is_body_visible(
    env: &mut JNIEnv<'_>,
    _renderer: &JObject<'_>,
    state: &JObject<'_>,
) -> bool {
    if should_replace_avatar_body(env, state) {
        BODY_VISIBLE_SKIP.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    !read_bool_field(env, state, &["isInvisible"])
}


pub fn on_avatar_body(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    state: &JObject<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    camera: &JObject<'_>,
) -> bool {
    DISPATCHER_HITS.fetch_add(1, Ordering::Relaxed);
    if pose_stack.is_null() || collector.is_null() {
        return false;
    }
    let result = (|| {
        if should_replace_avatar_body(env, state) {
            let Some(player) = resolve_player_uuid(env, state) else {
                return false;
            };
            let Some(avatar_id) = runtime::active_avatar_id(player) else {
                return false;
            };
            if !submit_avatar_mesh(
                env, pose_stack, collector, state, renderer, player, &avatar_id,
            ) {
                return false;
            }
            DISPATCHER_REPLACED.fetch_add(1, Ordering::Relaxed);
            let hits = DISPATCHER_HITS.load(Ordering::Relaxed);
            let reps = DISPATCHER_REPLACED.load(Ordering::Relaxed);
            
            native_jvmti::status::update(|status| {
                if status.last_error.is_empty() {
                    status.last_error = format!("tp hits={hits} replaced={reps}");
                }
            });
            return true;
        }
        
        forward_name_display(env, renderer, state, pose_stack, collector, camera);
        false
    })();
    jni_bridge::clear_pending_exception(env);
    result
}

fn forward_name_display(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    state: &JObject<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    camera: &JObject<'_>,
) {
    if renderer.is_null() || state.is_null() || camera.is_null() {
        return;
    }
    let show_ears = read_bool_field(env, state, &["showExtraEars"]);
    let offset = if show_ears { -10 } else { 0 };
    
    
    let result = env.call_method(
        renderer,
        "submitNameDisplay",
        "(Lnet/minecraft/client/renderer/entity/state/EntityRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;Lnet/minecraft/client/renderer/state/level/CameraRenderState;I)V",
        &[
            JValue::Object(state),
            JValue::Object(pose_stack),
            JValue::Object(collector),
            JValue::Object(camera),
            JValue::Int(offset),
        ],
    );
    if result.is_err() {
        jni_bridge::clear_pending_exception(env);
    }
}

fn should_replace_avatar_body(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> bool {
    if !replacement_enabled() || !client_in_world(env) || state.is_null() {
        return false;
    }
    match layer_policy(RenderContext::ThirdPersonBody, true) {
        LayerPolicy::Passthrough | LayerPolicy::Suppress => return false,
        LayerPolicy::Replace => {}
    }
    let Some(player) = resolve_player_uuid(env, state) else {
        return false;
    };
    let Some(avatar_id) = runtime::active_avatar_id(player) else {
        return false;
    };
    !(avatar_id == DEMO_CUBE_ID && !runtime::player_has_explicit_avatar(player))
}



pub fn on_dispatcher_submit(
    env: &mut JNIEnv<'_>,
    _dispatcher: &JObject<'_>,
    state: &JObject<'_>,
    _camera: &JObject<'_>,
    x: f64,
    y: f64,
    z: f64,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
) -> bool {
    DISPATCHER_HITS.fetch_add(1, Ordering::Relaxed);
    if !should_replace_avatar_body(env, state) || pose_stack.is_null() || collector.is_null() {
        return false;
    }
    let Some(player) = resolve_player_uuid(env, state) else {
        return false;
    };
    let Some(avatar_id) = runtime::active_avatar_id(player) else {
        return false;
    };

    let Ok(local_stack) = pose_stack_copy(env, pose_stack) else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    if env
        .call_method(
            &local_stack,
            "translate",
            "(DDD)V",
            &[JValue::Double(x), JValue::Double(y), JValue::Double(z)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        return false;
    }

    let replaced = submit_avatar_mesh(
        env,
        &local_stack,
        collector,
        state,
        &JObject::null(),
        player,
        &avatar_id,
    );
    if replaced {
        DISPATCHER_REPLACED.fetch_add(1, Ordering::Relaxed);
    }
    replaced
}


pub fn on_living_submit(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    state: &JObject<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    _camera: &JObject<'_>,
) -> bool {
    LIVING_HITS.fetch_add(1, Ordering::Relaxed);
    if !replacement_enabled() {
        set_primary_body_status("disabled");
        return false;
    }
    if !client_in_world(env) {
        set_primary_body_status("no-world");
        return false;
    }
    if state.is_null() || pose_stack.is_null() || collector.is_null() {
        set_primary_body_status("null-args");
        return false;
    }

    match layer_policy(RenderContext::ThirdPersonBody, true) {
        LayerPolicy::Passthrough | LayerPolicy::Suppress => {
            set_primary_body_status("permission");
            return false;
        }
        LayerPolicy::Replace => {}
    }

    let Some(player) = resolve_player_uuid(env, state) else {
        set_primary_body_status("not-local-player");
        return false;
    };

    let Some(avatar_id) = runtime::active_avatar_id(player) else {
        set_primary_body_status("no-active-avatar");
        return false;
    };

    
    if avatar_id == DEMO_CUBE_ID && !runtime::player_has_explicit_avatar(player) {
        set_primary_body_status("demo-fallback");
        return false;
    }

    let replaced = submit_avatar_mesh(
        env, pose_stack, collector, state, renderer, player, &avatar_id,
    );
    if replaced {
        LIVING_REPLACED.fetch_add(1, Ordering::Relaxed);
        set_primary_body_status("submitted");
    } else {
        set_primary_body_status("submit-failed");
    }
    replaced
}

fn set_primary_body_status(reason: &str) {
    static LAST: OnceLock<Mutex<String>> = OnceLock::new();
    let Ok(mut last) = LAST.get_or_init(|| Mutex::new(String::new())).try_lock() else {
        return;
    };
    if last.as_str() == reason {
        return;
    }
    *last = reason.to_string();
    native_jvmti::status::set_error(format!(
        "primary-body {reason} hits={} submitted={}",
        LIVING_HITS.load(Ordering::Relaxed),
        LIVING_REPLACED.load(Ordering::Relaxed)
    ));
}

pub fn on_first_person_hand(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    _light: i32,
    right: bool,
) -> bool {
    jni_bridge::clear_pending_exception(env);
    if !replacement_enabled()
        || !client_in_world(env)
        || pose_stack.is_null()
        || collector.is_null()
    {
        return false;
    }
    match layer_policy(RenderContext::FirstPersonHand, true) {
        LayerPolicy::Passthrough | LayerPolicy::Suppress => return false,
        LayerPolicy::Replace => {}
    }
    let Some(player) = local_player_uuid(env) else {
        return false;
    };
    let Some(avatar_id) = runtime::active_avatar_id(player) else {
        return false;
    };
    if avatar_id == DEMO_CUBE_ID && !runtime::player_has_explicit_avatar(player) {
        return false;
    }

    prepare_avatar_textures(env, &avatar_id);
    let arm = if right {
        crate::avatar::model::ParentType::RightArm
    } else {
        crate::avatar::model::ParentType::LeftArm
    };
    let parts = textured_meshes_for_arm(&avatar_id, arm, 0.0);
    if parts.is_empty() {
        
        return false;
    }

    
    
    
    
    let Ok(local_stack) = pose_stack_copy(env, pose_stack) else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    
    let z_rest = if right { 0.1_f32 } else { -0.1_f32 };
    let _ = rotate_axis_radians(env, &local_stack, "ZP", z_rest);
    
    if env
        .call_method(
            &local_stack,
            "translate",
            "(FFF)V",
            &[JValue::Float(0.0), JValue::Float(1.5), JValue::Float(0.0)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
    }
    let _ = rotate_axis_degrees(env, &local_stack, "ZP", 180.0);

    let mut any = false;
    for (texture_index, handle) in parts {
        match submit_custom_geometry(
            env,
            &local_stack,
            collector,
            handle,
            &avatar_id,
            texture_index,
        ) {
            Ok(()) => any = true,
            Err(error) => {
                native_jvmti::status::set_error(format!("fp hand: {error}"));
            }
        }
    }
    jni_bridge::clear_pending_exception(env);
    if any {
        native_jvmti::status::update(|status| {
            if status.last_error.starts_with("ok ")
                || status.last_error.starts_with("anim ")
                || status.last_error.starts_with("fp ")
                || status.last_error.is_empty()
            {
                status.last_error =
                    format!("fp v17 {} arm ok", if right { "right" } else { "left" });
            }
        });
    }
    any
}


fn apply_model_part_rotations(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    pose_stack: &JObject<'_>,
    right: bool,
) {
    if renderer.is_null() {
        return;
    }
    let model = match env.get_field(
        renderer,
        "model",
        "Lnet/minecraft/client/model/EntityModel;",
    ) {
        Ok(v) => match v.l() {
            Ok(m) if !m.is_null() => m,
            _ => return,
        },
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return;
        }
    };
    let part_name = if right { "rightArm" } else { "leftArm" };
    let part = match env.get_field(
        &model,
        part_name,
        "Lnet/minecraft/client/model/geom/ModelPart;",
    ) {
        Ok(v) => match v.l() {
            Ok(p) if !p.is_null() => p,
            _ => return,
        },
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return;
        }
    };
    let x_rot = read_float_field(env, &part, &["xRot"], 0.0);
    let y_rot = read_float_field(env, &part, &["yRot"], 0.0);
    let z_rot = read_float_field(env, &part, &["zRot"], 0.0);
    let _ = rotate_axis_radians(env, pose_stack, "ZP", z_rot);
    let _ = rotate_axis_radians(env, pose_stack, "YP", y_rot);
    let _ = rotate_axis_radians(env, pose_stack, "XP", x_rot);
}

fn rotate_axis_radians(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    axis: &str,
    radians: f32,
) -> Result<(), String> {
    if radians.abs() < 1e-8 {
        return Ok(());
    }
    let axis_obj = env
        .get_static_field("com/mojang/math/Axis", axis, "Lcom/mojang/math/Axis;")
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let quat = env
        .call_method(
            axis_obj,
            "rotation",
            "(F)Lorg/joml/Quaternionf;",
            &[JValue::Float(radians)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    if env
        .call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionfc;)V",
            &[JValue::Object(&quat)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        env.call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionf;)V",
            &[JValue::Object(&quat)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn on_geometry(env: &mut JNIEnv<'_>, pose: &JObject<'_>, consumer: &JObject<'_>, handle: i64) {
    let key = handle as u64;
    let Ok(guard) = meshes().lock() else {
        return;
    };
    let Some(mesh) = guard.get(&key) else {
        return;
    };
    let generation = mesh_generations()
        .lock()
        .ok()
        .and_then(|generations| generations.get(&key).copied())
        .unwrap_or(0);
    emit_mesh(env, pose, consumer, key, generation, mesh);
}

fn submit_avatar_mesh(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    state: &JObject<'_>,
    renderer: &JObject<'_>,
    _player: Uuid,
    avatar_id: &str,
) -> bool {
    
    env.with_local_frame(256, |env| {
        Ok::<bool, jni::errors::Error>(submit_avatar_mesh_inner(
            env, pose_stack, collector, state, renderer, _player, avatar_id,
        ))
    })
    .unwrap_or(false)
}

fn submit_avatar_mesh_inner(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    state: &JObject<'_>,
    renderer: &JObject<'_>,
    _player: Uuid,
    avatar_id: &str,
) -> bool {
    prepare_avatar_textures(env, avatar_id);

    
    
    
    let Ok(local_stack) = pose_stack_copy(env, pose_stack) else {
        jni_bridge::clear_pending_exception(env);
        native_jvmti::status::set_error("pose_stack_copy failed");
        return false;
    };

    
    
    
    
    
    
    
    
    
    let body_rot = read_body_yaw_degrees(env, state);
    let _ = rotate_yaw(env, &local_stack, 180.0 - body_rot);
    apply_living_entity_model_frame(env, &local_stack);
    
    if env
        .call_method(
            &local_stack,
            "translate",
            "(FFF)V",
            &[JValue::Float(0.0), JValue::Float(1.5), JValue::Float(0.0)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
    }
    let _ = rotate_axis_degrees(env, &local_stack, "ZP", 180.0);

    let vanilla = read_vanilla_pose(env, renderer, state);
    let bake_key = BodyBakeKey {
        age_bits: render_age_bits(env, state),
        vanilla_fingerprint: vanilla.bake_fingerprint(),
    };
    let anim_dt = animation_dt(env, state);
    let walk_speed = read_float_field(env, state, &["walkAnimationSpeed"], 0.0);
    let crouching = read_bool_field(env, state, &["isCrouching", "isInSneakingPose"]);
    let sprinting = read_bool_field(env, state, &["isSprinting"]);
    
    
    let gliding = read_bool_field(env, state, &["isFallFlying", "isGliding"])
        || read_float_field(env, state, &["fallFlyingTimeInTicks", "glidingTicks"], 0.0) > 0.0;
    runtime::sync_movement_clips(avatar_id, walk_speed, sprinting, crouching, gliding);

    let pose_dbg = (
        vanilla.magnitude(),
        vanilla.head.rot.x,
        vanilla.head.rot.y,
        vanilla.left_arm.rot.x,
        vanilla.right_arm.rot.x,
    );
    
    maybe_fire_render(env, state, _player);
    maybe_fire_tick(env, state, _player);

    let parts = textured_meshes_for_avatar(avatar_id, false, Some(vanilla), bake_key, anim_dt);
    {
        use std::sync::atomic::{AtomicU32, Ordering};
        static FRAME: AtomicU32 = AtomicU32::new(0);
        if FRAME.fetch_add(1, Ordering::Relaxed) % 15 == 0 {
            let (bucket_count, vertex_count) = mesh_stats(&parts);
            native_jvmti::status::update(|status| {
                if status.last_error.starts_with("tp ")
                    || status.last_error.starts_with("ok ")
                    || status.last_error.starts_with("anim ")
                    || status.last_error.starts_with("fp ")
                    || status.last_error.is_empty()
                {
                    
                    status.last_error = format!(
                        "anim v33 live={}/{} buckets={} verts={} emit={}/{} skipBody={} mag={:.1}",
                        LIVING_REPLACED.load(Ordering::Relaxed),
                        LIVING_HITS.load(Ordering::Relaxed),
                        bucket_count,
                        vertex_count,
                        EMIT_PACKED_OK.load(Ordering::Relaxed),
                        EMIT_PACKED_FAIL.load(Ordering::Relaxed),
                        BODY_VISIBLE_SKIP.load(Ordering::Relaxed),
                        pose_dbg.0
                    );
                }
            });
        }
    }
    let mut any = false;
    for (texture_index, handle) in parts {
        match submit_custom_geometry(
            env,
            &local_stack,
            collector,
            handle,
            avatar_id,
            texture_index,
        ) {
            Ok(()) => any = true,
            Err(error) => {
                native_jvmti::status::set_error(format!("submitCustomGeometry: {error}"));
            }
        }
    }
    jni_bridge::clear_pending_exception(env);
    any
}


fn read_bool_field(env: &mut JNIEnv<'_>, obj: &JObject<'_>, names: &[&str]) -> bool {
    for name in names {
        match env.get_field(obj, name, "Z") {
            Ok(v) => {
                if let Ok(b) = v.z() {
                    return b;
                }
            }
            Err(_) => jni_bridge::clear_pending_exception(env),
        }
    }
    false
}

fn read_float_field(env: &mut JNIEnv<'_>, obj: &JObject<'_>, names: &[&str], default: f32) -> f32 {
    for name in names {
        match env.get_field(obj, name, "F") {
            Ok(v) => {
                if let Ok(f) = v.f() {
                    return f;
                }
            }
            Err(_) => jni_bridge::clear_pending_exception(env),
        }
    }
    default
}



fn pose_stack_copy<'local>(
    env: &mut JNIEnv<'local>,
    source: &JObject<'_>,
) -> Result<JObject<'local>, String> {
    let copy = env
        .new_object("com/mojang/blaze3d/vertex/PoseStack", "()V", &[])
        .map_err(|e| e.to_string())?;
    if env.call_method(&copy, "pushPose", "()V", &[]).is_err() {
        jni_bridge::clear_pending_exception(env);
        return Err("pushPose on copy failed".into());
    }
    let src_last = env
        .call_method(
            source,
            "last",
            "()Lcom/mojang/blaze3d/vertex/PoseStack$Pose;",
            &[],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let dst_last = env
        .call_method(
            &copy,
            "last",
            "()Lcom/mojang/blaze3d/vertex/PoseStack$Pose;",
            &[],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    if env
        .call_method(
            &dst_last,
            "set",
            "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;)V",
            &[JValue::Object(&src_last)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        
        copy_pose_matrices(env, &src_last, &dst_last)?;
    }
    Ok(copy)
}

fn copy_pose_matrices(
    env: &mut JNIEnv<'_>,
    src: &JObject<'_>,
    dst: &JObject<'_>,
) -> Result<(), String> {
    let src_pose = env
        .call_method(src, "pose", "()Lorg/joml/Matrix4f;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let dst_pose = env
        .call_method(dst, "pose", "()Lorg/joml/Matrix4f;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    env.call_method(
        &dst_pose,
        "set",
        "(Lorg/joml/Matrix4fc;)Lorg/joml/Matrix4f;",
        &[JValue::Object(&src_pose)],
    )
    .or_else(|_| {
        jni_bridge::clear_pending_exception(env);
        env.call_method(
            &dst_pose,
            "set",
            "(Lorg/joml/Matrix4f;)Lorg/joml/Matrix4f;",
            &[JValue::Object(&src_pose)],
        )
    })
    .map_err(|e| e.to_string())?;

    let src_n = env
        .call_method(src, "normal", "()Lorg/joml/Matrix3f;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let dst_n = env
        .call_method(dst, "normal", "()Lorg/joml/Matrix3f;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    env.call_method(
        &dst_n,
        "set",
        "(Lorg/joml/Matrix3fc;)Lorg/joml/Matrix3f;",
        &[JValue::Object(&src_n)],
    )
    .or_else(|_| {
        jni_bridge::clear_pending_exception(env);
        env.call_method(
            &dst_n,
            "set",
            "(Lorg/joml/Matrix3f;)Lorg/joml/Matrix3f;",
            &[JValue::Object(&src_n)],
        )
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}


fn read_body_yaw_degrees(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> f32 {
    let v = read_float_field(env, state, &["bodyRot", "yRot"], 0.0);
    if v.is_finite() { v } else { 0.0 }
}


fn apply_living_entity_model_frame(env: &mut JNIEnv<'_>, pose_stack: &JObject<'_>) {
    if env
        .call_method(
            pose_stack,
            "scale",
            "(FFF)V",
            &[JValue::Float(-1.0), JValue::Float(-1.0), JValue::Float(1.0)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
    }
    if env
        .call_method(
            pose_stack,
            "translate",
            "(FFF)V",
            &[
                JValue::Float(0.0),
                JValue::Float(-1.501),
                JValue::Float(0.0),
            ],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
    }
}

fn rotate_axis_degrees(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    axis_name: &str,
    degrees: f32,
) -> Result<(), String> {
    let axis = env
        .get_static_field("com/mojang/math/Axis", axis_name, "Lcom/mojang/math/Axis;")
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let quat = env
        .call_method(
            axis,
            "rotationDegrees",
            "(F)Lorg/joml/Quaternionf;",
            &[JValue::Float(degrees)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    if env
        .call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionfc;)V",
            &[JValue::Object(&quat)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        env.call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionf;)V",
            &[JValue::Object(&quat)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn rotate_yaw(env: &mut JNIEnv<'_>, pose_stack: &JObject<'_>, degrees: f32) -> Result<(), String> {
    let axis = env
        .get_static_field("com/mojang/math/Axis", "YP", "Lcom/mojang/math/Axis;")
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let quat = env
        .call_method(
            axis,
            "rotationDegrees",
            "(F)Lorg/joml/Quaternionf;",
            &[JValue::Float(degrees)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    if env
        .call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionfc;)V",
            &[JValue::Object(&quat)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        env.call_method(
            pose_stack,
            "mulPose",
            "(Lorg/joml/Quaternionf;)V",
            &[JValue::Object(&quat)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn prepare_avatar_textures(env: &mut JNIEnv<'_>, avatar_id: &str) {
    use std::collections::HashSet;
    static READY: OnceLock<Mutex<(u64, HashSet<String>)>> = OnceLock::new();
    let content_gen = runtime::avatar_content_generation();
    if let Ok(mut guard) = READY.get_or_init(|| Mutex::new((0, HashSet::new()))).lock() {
        if guard.0 != content_gen {
            guard.0 = content_gen;
            guard.1.clear();
        }
        if guard.1.contains(avatar_id) {
            return;
        }
    }

    
    let jobs: Vec<(u32, Vec<u8>)> = runtime::with_avatar(avatar_id, |avatar| {
        if avatar.package.models.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for model in &avatar.package.models {
            for (index, tex) in model.textures.iter().enumerate() {
                if let Some(png) = tex.png_bytes.as_ref() {
                    let key_idx = index as u32;
                    if textures::has_texture(avatar_id, key_idx) {
                        continue;
                    }
                    out.push((key_idx, png.clone()));
                }
            }
        }
        out
    })
    .unwrap_or_default();

    let models_empty =
        runtime::with_avatar(avatar_id, |a| a.package.models.is_empty()).unwrap_or(true);
    if models_empty {
        native_jvmti::status::set_error(format!(
            "{avatar_id}: 0 bbmodels loaded — click Reload (parse/euler fix)"
        ));
        return;
    }

    for (index, png) in &jobs {
        if let Err(error) = textures::register_png_for(env, avatar_id, *index, png) {
            native_jvmti::status::set_error(format!("texture register: {error}"));
            return;
        }
    }
    
    
    let all_ok = runtime::with_avatar(avatar_id, |avatar| {
        avatar.package.models.iter().all(|model| {
            model.textures.iter().enumerate().all(|(i, tex)| {
                tex.png_bytes.is_none() || textures::has_texture(avatar_id, i as u32)
            })
        })
    })
    .unwrap_or(false);
    if all_ok {
        if let Ok(mut guard) = READY.get().unwrap().lock() {
            guard.1.insert(avatar_id.to_string());
        }
        
        native_jvmti::status::update(|status| {
            if status.last_error.starts_with("texpx ") {
                return;
            }
            if status.last_error.contains("texture")
                || status.last_error.starts_with("ok ")
                || status.last_error.starts_with("anim ")
                || status.last_error.is_empty()
            {
                status.last_error = format!("tex ok {}", jobs.len());
            }
        });
    }
}

struct AvatarDrawCache {
    avatar_id: String,
    content_gen: u64,
    tree: crate::avatar::model::ModelPart,
    body_key: Option<BodyBakeKey>,
    
    handles: Vec<(Option<u32>, i64)>,
    left_arm_handles: Vec<(Option<u32>, i64)>,
    right_arm_handles: Vec<(Option<u32>, i64)>,
    left_arm_baked: bool,
    right_arm_baked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BodyBakeKey {
    age_bits: u32,
    vanilla_fingerprint: u64,
}

fn draw_cache() -> &'static Mutex<Option<AvatarDrawCache>> {
    static CACHE: OnceLock<Mutex<Option<AvatarDrawCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn ensure_draw_tree(avatar_id: &str) -> bool {
    let content_gen = runtime::avatar_content_generation();
    let Ok(mut slot) = draw_cache().lock() else {
        return false;
    };
    if let Some(cache) = slot.as_ref() {
        if cache.avatar_id == avatar_id && cache.content_gen == content_gen {
            return true;
        }
    }
    let tree = runtime::with_avatar(avatar_id, |avatar| {
        avatar
            .package
            .models
            .first()
            .map(crate::avatar::model::build_model_tree)
    })
    .flatten();
    let Some(tree) = tree else {
        *slot = None;
        return false;
    };
    *slot = Some(AvatarDrawCache {
        avatar_id: avatar_id.to_string(),
        content_gen,
        tree,
        body_key: None,
        handles: Vec::new(),
        left_arm_handles: Vec::new(),
        right_arm_handles: Vec::new(),
        left_arm_baked: false,
        right_arm_baked: false,
    });
    true
}

fn bake_tree_handles(
    tree: &crate::avatar::model::ModelPart,
    handles: &mut Vec<(Option<u32>, i64)>,
) -> Vec<(Option<u32>, i64)> {
    let Ok(mut guard) = meshes().lock() else {
        return Vec::new();
    };
    crate::avatar::model::flatten_into_mesh_map(tree, handles, &mut guard, || {
        NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
    });
    mark_mesh_handles_dirty(handles);
    handles.clone()
}

fn mark_mesh_handles_dirty(handles: &[(Option<u32>, i64)]) {
    if let Ok(mut generations) = mesh_generations().lock() {
        for (_, handle) in handles {
            let key = *handle as u64;
            let entry = generations.entry(key).or_insert(0);
            *entry = entry.wrapping_add(1).max(1);
        }
    }
}

fn mesh_stats(handles: &[(Option<u32>, i64)]) -> (usize, usize) {
    let Ok(guard) = meshes().lock() else {
        return (handles.len(), 0);
    };
    let vertices = handles
        .iter()
        .filter_map(|(_, handle)| guard.get(&(*handle as u64)))
        .map(|mesh| mesh.vertices.len())
        .sum();
    (handles.len(), vertices)
}


fn textured_meshes_for_avatar(
    avatar_id: &str,
    _hand: bool,
    vanilla: Option<crate::avatar::model::VanillaPose>,
    bake_key: BodyBakeKey,
    anim_dt: f32,
) -> Vec<(Option<u32>, i64)> {
    if avatar_id == DEMO_CUBE_ID {
        return vec![(None, store_mesh(Mesh::demo_cube()))];
    }
    if !ensure_draw_tree(avatar_id) {
        return vec![(None, store_mesh(Mesh::demo_cube()))];
    }
    let Ok(mut slot) = draw_cache().lock() else {
        return vec![(None, store_mesh(Mesh::demo_cube()))];
    };
    let Some(cache) = slot.as_mut() else {
        return vec![(None, store_mesh(Mesh::demo_cube()))];
    };

    if cache.body_key == Some(bake_key) && !cache.handles.is_empty() {
        return cache.handles.clone();
    }

    cache.tree.reset_animation();
    runtime::overlay_script_transforms(avatar_id, &mut cache.tree);
    runtime::apply_playing_animations(avatar_id, &mut cache.tree, anim_dt);
    if let Some(pose) = vanilla.as_ref() {
        crate::avatar::model::apply_vanilla_pose(&mut cache.tree, pose);
    }
    crate::avatar::model::recompute_world_matrices(&mut cache.tree);
    let out = bake_tree_handles(&cache.tree, &mut cache.handles);
    if out.is_empty() {
        let fallback = runtime::with_avatar(avatar_id, |avatar| {
            avatar
                .package
                .models
                .first()
                .map(mesh::meshes_from_model)
                .unwrap_or_default()
        })
        .unwrap_or_default();
        if fallback.is_empty() {
            return vec![(None, store_mesh(Mesh::demo_cube()))];
        }
        cache.handles.clear();
        if let Ok(mut guard) = meshes().lock() {
            for tm in fallback {
                let h = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
                guard.insert(h, tm.mesh);
                cache.handles.push((tm.texture_index, h as i64));
            }
        }
        mark_mesh_handles_dirty(&cache.handles);
        cache.body_key = Some(bake_key);
        return cache.handles.clone();
    }
    cache.body_key = Some(bake_key);
    out
}


fn textured_meshes_for_arm(
    avatar_id: &str,
    arm: crate::avatar::model::ParentType,
    anim_dt: f32,
) -> Vec<(Option<u32>, i64)> {
    if !ensure_draw_tree(avatar_id) {
        return Vec::new();
    }
    let Ok(mut slot) = draw_cache().lock() else {
        return Vec::new();
    };
    let Some(cache) = slot.as_mut() else {
        return Vec::new();
    };

    let cached = match arm {
        crate::avatar::model::ParentType::LeftArm => {
            cache.left_arm_baked && !cache.left_arm_handles.is_empty()
        }
        _ => cache.right_arm_baked && !cache.right_arm_handles.is_empty(),
    };
    if cached {
        return match arm {
            crate::avatar::model::ParentType::LeftArm => cache.left_arm_handles.clone(),
            _ => cache.right_arm_handles.clone(),
        };
    }

    cache.tree.reset_animation();
    runtime::overlay_script_transforms(avatar_id, &mut cache.tree);
    runtime::apply_playing_animations(avatar_id, &mut cache.tree, anim_dt);
    crate::avatar::model::recompute_world_matrices(&mut cache.tree);
    let (handles, tree) = match arm {
        crate::avatar::model::ParentType::LeftArm => (&mut cache.left_arm_handles, &cache.tree),
        _ => (&mut cache.right_arm_handles, &cache.tree),
    };
    let Some(arm_part) = crate::avatar::model::find_part_by_parent_type(tree, arm) else {
        handles.clear();
        return Vec::new();
    };
    let out = bake_tree_handles(arm_part, handles);
    match arm {
        crate::avatar::model::ParentType::LeftArm => cache.left_arm_baked = true,
        _ => cache.right_arm_baked = true,
    }
    out
}

fn render_age_bits(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> u32 {
    read_float_field(env, state, &["ageInTicks"], 0.0).to_bits()
}


fn animation_dt(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> f32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static LAST_AGE_BITS: AtomicU32 = AtomicU32::new(0);
    let age = read_float_field(env, state, &["ageInTicks"], 0.0);
    let prev = f32::from_bits(LAST_AGE_BITS.swap(age.to_bits(), Ordering::Relaxed));
    let delta_ticks = age - prev;
    if !delta_ticks.is_finite() || delta_ticks.abs() < 1e-6 {
        0.0
    } else {
        (delta_ticks / 20.0).clamp(0.0, 0.25)
    }
}

fn read_vanilla_pose(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    state: &JObject<'_>,
) -> crate::avatar::model::VanillaPose {
    
    if let Some(pose) = read_pose_from_player_model(env, renderer, state) {
        return pose;
    }
    let crouching = read_bool_field(env, state, &["isCrouching", "isInSneakingPose"]);
    let x_rot = read_float_field(env, state, &["xRot"], 0.0);
    let y_rot = read_float_field(env, state, &["yRot"], 0.0);
    let walk_pos = read_float_field(env, state, &["walkAnimationPos"], 0.0);
    let walk_speed = read_float_field(env, state, &["walkAnimationSpeed"], 0.0);
    let age = read_float_field(env, state, &["ageInTicks"], 0.0);
    let speed_value = {
        let v = read_float_field(env, state, &["speedValue"], 1.0);
        if v.abs() < 1e-3 { 1.0 } else { v }
    };
    crate::avatar::model::VanillaPose::from_render_state(
        x_rot,
        y_rot,
        walk_pos,
        walk_speed,
        speed_value,
        crouching,
        age,
    )
}


fn read_pose_from_player_model(
    env: &mut JNIEnv<'_>,
    renderer: &JObject<'_>,
    state: &JObject<'_>,
) -> Option<crate::avatar::model::VanillaPose> {
    if renderer.is_null() {
        return None;
    }
    let model = match env.get_field(
        renderer,
        "model",
        "Lnet/minecraft/client/model/EntityModel;",
    ) {
        Ok(v) => v.l().ok()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return None;
        }
    };
    if model.is_null() {
        return None;
    }

    if env
        .call_method(
            &model,
            "setupAnim",
            "(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;)V",
            &[JValue::Object(state)],
        )
        .is_err()
    {
        jni_bridge::clear_pending_exception(env);
        let _ = env.call_method(
            &model,
            "setupAnim",
            "(Ljava/lang/Object;)V",
            &[JValue::Object(state)],
        );
        jni_bridge::clear_pending_exception(env);
    }

    let part_sig = "Lnet/minecraft/client/model/geom/ModelPart;";
    let fields = model_part_fields(env)?;
    let read_part =
        |env: &mut JNIEnv<'_>, name: &str| -> Option<crate::avatar::model::VanillaPartData> {
            let part = match env.get_field(&model, name, part_sig) {
                Ok(v) => v.l().ok()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(env);
                    return None;
                }
            };
            if part.is_null() {
                return None;
            }
            let g = |env: &mut JNIEnv<'_>, fid: JFieldID, default: f32| {
                read_float_field_id(env, &part, fid, default)
            };
            let x = g(env, fields.x, 0.0);
            let y = g(env, fields.y, 0.0);
            let z = g(env, fields.z, 0.0);
            let x_rot = g(env, fields.x_rot, 0.0);
            let y_rot = g(env, fields.y_rot, 0.0);
            let z_rot = g(env, fields.z_rot, 0.0);
            let x_scale = g(env, fields.x_scale, 1.0);
            let y_scale = g(env, fields.y_scale, 1.0);
            let z_scale = g(env, fields.z_scale, 1.0);
            let visible = match unsafe {
                env.get_field_unchecked(
                    &part,
                    fields.visible,
                    ReturnType::Primitive(Primitive::Boolean),
                )
            } {
                Ok(v) => v.z().unwrap_or(true),
                Err(_) => {
                    jni_bridge::clear_pending_exception(env);
                    true
                }
            };
            Some(crate::avatar::model::VanillaPose::from_model_part(
                x, y, z, x_rot, y_rot, z_rot, x_scale, y_scale, z_scale, visible,
            ))
        };

    let head = read_part(env, "head")?;
    Some(crate::avatar::model::VanillaPose {
        head,
        body: read_part(env, "body").unwrap_or_default(),
        left_arm: read_part(env, "leftArm").unwrap_or_default(),
        right_arm: read_part(env, "rightArm").unwrap_or_default(),
        left_leg: read_part(env, "leftLeg").unwrap_or_default(),
        right_leg: read_part(env, "rightLeg").unwrap_or_default(),
    })
}

#[derive(Clone, Copy)]
struct ModelPartFieldIds {
    x: JFieldID,
    y: JFieldID,
    z: JFieldID,
    x_rot: JFieldID,
    y_rot: JFieldID,
    z_rot: JFieldID,
    x_scale: JFieldID,
    y_scale: JFieldID,
    z_scale: JFieldID,
    visible: JFieldID,
}

fn model_part_fields(env: &mut JNIEnv<'_>) -> Option<ModelPartFieldIds> {
    static IDS: OnceLock<ModelPartFieldIds> = OnceLock::new();
    if let Some(ids) = IDS.get() {
        return Some(*ids);
    }
    let class = env
        .find_class("net/minecraft/client/model/geom/ModelPart")
        .ok()?;
    let ids = ModelPartFieldIds {
        x: env.get_field_id(&class, "x", "F").ok()?,
        y: env.get_field_id(&class, "y", "F").ok()?,
        z: env.get_field_id(&class, "z", "F").ok()?,
        x_rot: env.get_field_id(&class, "xRot", "F").ok()?,
        y_rot: env.get_field_id(&class, "yRot", "F").ok()?,
        z_rot: env.get_field_id(&class, "zRot", "F").ok()?,
        x_scale: env.get_field_id(&class, "xScale", "F").ok()?,
        y_scale: env.get_field_id(&class, "yScale", "F").ok()?,
        z_scale: env.get_field_id(&class, "zScale", "F").ok()?,
        visible: env.get_field_id(&class, "visible", "Z").ok()?,
    };
    let _ = IDS.set(ids);
    Some(ids)
}

fn read_float_field_id(
    env: &mut JNIEnv<'_>,
    obj: &JObject<'_>,
    fid: JFieldID,
    default: f32,
) -> f32 {
    match unsafe { env.get_field_unchecked(obj, fid, ReturnType::Primitive(Primitive::Float)) } {
        Ok(v) => v.f().unwrap_or(default),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            default
        }
    }
}

fn maybe_fire_tick(env: &mut JNIEnv<'_>, state: &JObject<'_>, player: Uuid) {
    use std::sync::atomic::{AtomicI32, Ordering};
    static LAST_TICK: AtomicI32 = AtomicI32::new(i32::MIN);
    let age = read_float_field(env, state, &["ageInTicks"], 0.0) as i32;
    let prev = LAST_TICK.load(Ordering::Relaxed);
    if age != prev {
        LAST_TICK.store(age, Ordering::Relaxed);
        let _ = runtime::fire_lua_event(player, "TICK");
    }
}

fn maybe_fire_render(env: &mut JNIEnv<'_>, state: &JObject<'_>, player: Uuid) {
    if !runtime::player_has_lua(player) {
        return;
    }
    use std::sync::atomic::{AtomicI32, Ordering};
    static LAST_RENDER: AtomicI32 = AtomicI32::new(i32::MIN);
    let age = read_float_field(env, state, &["ageInTicks"], 0.0) as i32;
    let prev = LAST_RENDER.load(Ordering::Relaxed);
    if age != prev {
        LAST_RENDER.store(age, Ordering::Relaxed);
        let _ = runtime::fire_lua_event(player, "RENDER");
    }
}

fn protected_mesh_handles() -> HashSet<u64> {
    let mut keep = HashSet::new();
    
    let Ok(slot) = draw_cache().try_lock() else {
        return keep;
    };
    if let Some(cache) = slot.as_ref() {
        for (_, h) in cache
            .handles
            .iter()
            .chain(cache.left_arm_handles.iter())
            .chain(cache.right_arm_handles.iter())
        {
            keep.insert(*h as u64);
        }
    }
    keep
}

fn store_mesh(mesh: Mesh) -> i64 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut guard) = meshes().lock() {
        guard.insert(handle, mesh);
        if let Ok(mut generations) = mesh_generations().lock() {
            generations.insert(handle, 1);
        }
        if guard.len() > 256 {
            let stale = handle.saturating_sub(200);
            let keep = protected_mesh_handles();
            guard.retain(|key, _| *key > stale || keep.contains(key));
            if let Ok(mut generations) = mesh_generations().lock() {
                generations.retain(|key, _| guard.contains_key(key));
            }
            if let Ok(mut packed) = packed_array_cache().lock() {
                packed.retain(|key, _| guard.contains_key(key));
            }
            
            if let Ok(mut renderers) = handle_renderers().lock() {
                renderers.retain(|h, _| guard.contains_key(&(*h as u64)));
            }
        }
    }
    handle as i64
}

fn submit_custom_geometry(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    handle: i64,
    avatar_id: &str,
    texture_index: Option<u32>,
) -> Result<(), String> {
    let render_type = mesh_render_type(env, avatar_id, texture_index)?;
    let renderer = geometry_renderer(env, handle)?;
    let result = env.call_method(
        collector,
        RENDER_SYMBOLS.submit_custom_geometry.name,
        RENDER_SYMBOLS.submit_custom_geometry.descriptor,
        &[
            JValue::Object(pose_stack),
            JValue::Object(&render_type),
            JValue::Object(&renderer),
        ],
    );
    if let Err(error) = result {
        jni_bridge::clear_pending_exception(env);
        let ordered = env
            .call_method(
                collector,
                "order",
                "(I)Lnet/minecraft/client/renderer/OrderedSubmitNodeCollector;",
                &[JValue::Int(0)],
            )
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        if ordered.is_null() {
            return Err(format!(
                "submitCustomGeometry failed ({error}) and order(0) was null"
            ));
        }
        env.call_method(
            ordered,
            RENDER_SYMBOLS.submit_custom_geometry.name,
            RENDER_SYMBOLS.submit_custom_geometry.descriptor,
            &[
                JValue::Object(pose_stack),
                JValue::Object(&render_type),
                JValue::Object(&renderer),
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    if env.exception_check().unwrap_or(false) {
        jni_bridge::clear_pending_exception(env);
        return Err("submitCustomGeometry raised a Java exception".into());
    }
    Ok(())
}

fn mesh_render_type<'local>(
    env: &mut JNIEnv<'local>,
    avatar_id: &str,
    texture_index: Option<u32>,
) -> Result<JObject<'local>, String> {
    let cache_key = match texture_index {
        Some(i) => format!("{avatar_id}:{i}"),
        None => format!("{avatar_id}:none"),
    };
    if let Ok(guard) = render_type_cache().lock() {
        if let Some(global) = guard.get(&cache_key) {
            if let Ok(local) = env.new_local_ref(global.as_obj()) {
                if !local.is_null() {
                    return Ok(local);
                }
            }
        }
    }

    let render_types = env
        .find_class("net/minecraft/client/renderer/rendertype/RenderTypes")
        .map_err(|error| error.to_string())?;

    let texture = if let Some(id) = textures::texture_identifier(env, avatar_id, texture_index) {
        id
    } else {
        env.call_static_method(
            "net/minecraft/resources/Identifier",
            "withDefaultNamespace",
            "(Ljava/lang/String;)Lnet/minecraft/resources/Identifier;",
            &[JValue::Object(
                &env.new_string("textures/block/white_wool.png")
                    .map_err(|e| e.to_string())?
                    .into(),
            )],
        )
        .map_err(|error| error.to_string())?
        .l()
        .map_err(|error| error.to_string())?
    };

    
    
    let candidates = [
        "entityCutout",
        "entityTranslucent",
        "entityTranslucentEmissive",
        "eyes",
        "entitySolid",
    ];
    let mut last_err = String::new();
    let mut created: Option<JObject<'local>> = None;
    for name in candidates {
        match env.call_static_method(
            &render_types,
            name,
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/rendertype/RenderType;",
            &[JValue::Object(&texture)],
        ) {
            Ok(value) => match value.l() {
                Ok(obj) if !obj.is_null() => {
                    created = Some(obj);
                    break;
                }
                Ok(_) => last_err = format!("{name} returned null"),
                Err(e) => last_err = e.to_string(),
            },
            Err(e) => {
                jni_bridge::clear_pending_exception(env);
                last_err = e.to_string();
            }
        }
    }
    let obj = created.ok_or_else(|| format!("no RenderType worked: {last_err}"))?;
    if let Ok(global) = env.new_global_ref(&obj) {
        if let Ok(mut guard) = render_type_cache().lock() {
            guard.insert(cache_key, global);
        }
    }
    Ok(obj)
}

fn geometry_renderer<'local>(
    env: &mut JNIEnv<'local>,
    handle: i64,
) -> Result<JObject<'local>, String> {
    
    
    if let Ok(guard) = handle_renderers().lock() {
        if let Some(global) = guard.get(&handle) {
            if let Ok(local) = env.new_local_ref(global.as_obj()) {
                if !local.is_null() {
                    return Ok(local);
                }
            }
        }
    }

    let class = env
        .find_class("xz/runtime/GeoMeshRenderer")
        .map_err(|error| error.to_string())?;
    let instance = env
        .alloc_object(&class)
        .map_err(|error| error.to_string())?;
    env.set_field(&instance, "handle", "J", JValue::Long(handle))
        .map_err(|error| error.to_string())?;
    if let Ok(global) = env.new_global_ref(&instance) {
        if let Ok(mut guard) = handle_renderers().lock() {
            guard.insert(handle, global);
            if guard.len() > 256 {
                let stale = handle.saturating_sub(200);
                guard.retain(|key, _| *key > stale);
            }
        }
    }
    Ok(instance)
}

pub fn store_geometry_renderer(global: GlobalRef) {
    if let Ok(mut guard) = GEOMETRY_RENDERER.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(global);
    }
}

pub fn store_mesh_emit_class(global: GlobalRef) {
    if let Ok(mut guard) = mesh_emit_class().lock() {
        *guard = Some(global);
        MESH_EMIT_READY.store(true, Ordering::Release);
    }
}

fn mesh_emit_class() -> &'static Mutex<Option<GlobalRef>> {
    static CLS: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();
    CLS.get_or_init(|| Mutex::new(None))
}

#[derive(Clone)]
struct PackedFloatArray {
    generation: u64,
    floats_len: usize,
    array: GlobalRef,
}

fn emit_mesh(
    env: &mut JNIEnv<'_>,
    pose: &JObject<'_>,
    consumer: &JObject<'_>,
    handle: u64,
    generation: u64,
    mesh: &Mesh,
) {
    if mesh.vertices.is_empty() {
        return;
    }
    if !MESH_EMIT_READY.load(Ordering::Acquire) {
        if EMIT_PACKED_FAIL.fetch_add(1, Ordering::Relaxed) == 0 {
            native_jvmti::status::update(|status| {
                if !status.last_error.starts_with("MeshEmit define failed:")
                    && !status.last_error.starts_with("MeshEmit global ref failed:")
                {
                    status.last_error =
                        "MeshEmit batch renderer unavailable; avatar draw skipped to protect FPS"
                            .to_string();
                }
            });
        }
        return;
    }
    if mesh
        .vertices
        .first()
        .is_some_and(|vertex| vertex.nx > 1.0 && vertex.ny == 0.0)
    {
        if emit_line_mesh_packed(env, pose, consumer, handle, generation, mesh) {
            EMIT_PACKED_OK.fetch_add(1, Ordering::Relaxed);
        }
        return;
    }
    
    if emit_mesh_packed(env, pose, consumer, handle, generation, mesh) {
        EMIT_PACKED_OK.fetch_add(1, Ordering::Relaxed);
        return;
    }
    if EMIT_PACKED_FAIL.fetch_add(1, Ordering::Relaxed) == 0 {
        native_jvmti::status::set_error(
            "MeshEmit batch renderer failed; avatar draw skipped to protect FPS",
        );
    }
}

fn emit_line_mesh_packed(
    env: &mut JNIEnv<'_>,
    pose: &JObject<'_>,
    consumer: &JObject<'_>,
    handle: u64,
    generation: u64,
    mesh: &Mesh,
) -> bool {
    
    let expected_len = mesh.vertices.len() * 8;
    if let Ok(guard) = packed_array_cache().lock()
        && let Some(cached) = guard.get(&handle)
        && cached.generation == generation
        && cached.floats_len == expected_len
        && let Ok(local) = env.new_local_ref(cached.array.as_obj())
        && !local.is_null()
    {
        return call_mesh_emit_method(
            env,
            "emitLines",
            pose,
            consumer,
            &local,
            mesh.vertices.len(),
        );
    }
    let mut packed = Vec::with_capacity(mesh.vertices.len() * 8);
    
    
    
    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let pair_start = index - (index % 2);
        let (from, to) = (
            mesh.vertices.get(pair_start),
            mesh.vertices.get(pair_start + 1),
        );
        let direction = match (from, to) {
            (Some(from), Some(to)) => {
                let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
                let length = (dx * dx + dy * dy + dz * dz).sqrt();
                if length > 1.0e-6 {
                    [dx / length, dy / length, dz / length]
                } else {
                    [0.0, 1.0, 0.0]
                }
            }
            _ => [0.0, 1.0, 0.0],
        };
        packed.extend_from_slice(&[
            vertex.x,
            vertex.y,
            vertex.z,
            f32::from_bits(
                ((vertex.a as u32) << 24)
                    | ((vertex.r as u32) << 16)
                    | ((vertex.g as u32) << 8)
                    | vertex.b as u32,
            ),
            vertex.nx,
            direction[0],
            direction[1],
            direction[2],
        ]);
    }
    let Ok(array) = env.new_float_array(packed.len() as i32) else {
        return false;
    };
    if env.set_float_array_region(&array, 0, &packed).is_err() {
        return false;
    }
    let array: JObject<'_> = array.into();
    if let Ok(global) = env.new_global_ref(&array)
        && let Ok(mut cache) = packed_array_cache().lock()
    {
        cache.insert(
            handle,
            PackedFloatArray {
                generation,
                floats_len: expected_len,
                array: global,
            },
        );
    }
    call_mesh_emit_method(
        env,
        "emitLines",
        pose,
        consumer,
        &array,
        mesh.vertices.len(),
    )
}

fn emit_mesh_packed(
    env: &mut JNIEnv<'_>,
    pose: &JObject<'_>,
    consumer: &JObject<'_>,
    handle: u64,
    generation: u64,
    mesh: &Mesh,
) -> bool {
    const STRIDE: usize = 9;
    let expected_len = mesh.vertices.len() * STRIDE;
    if let Ok(guard) = packed_array_cache().lock() {
        if let Some(cached) = guard.get(&handle) {
            if cached.generation == generation && cached.floats_len == expected_len {
                if let Ok(local) = env.new_local_ref(cached.array.as_obj()) {
                    if !local.is_null() {
                        return call_mesh_emit(env, pose, consumer, &local, mesh.vertices.len());
                    }
                }
            }
        }
    }

    thread_local! {
        static PACK: RefCell<Vec<f32>> = RefCell::new(Vec::new());
    }
    let packed_len = PACK.with(|cell| {
        let mut packed = cell.borrow_mut();
        packed.clear();
        packed.reserve(mesh.vertices.len() * STRIDE);
        for v in &mesh.vertices {
            packed.push(v.x);
            packed.push(v.y);
            packed.push(v.z);
            packed.push(v.u);
            packed.push(v.v);
            packed.push(v.nx);
            packed.push(v.ny);
            packed.push(v.nz);
            let color =
                ((v.a as u32) << 24) | ((v.r as u32) << 16) | ((v.g as u32) << 8) | v.b as u32;
            packed.push(f32::from_bits(color));
        }
        packed.len()
    });
    let Ok(arr) = env.new_float_array(packed_len as i32) else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    let set_ok = PACK.with(|cell| {
        let packed = cell.borrow();
        env.set_float_array_region(&arr, 0, &packed).is_ok()
    });
    if !set_ok {
        jni_bridge::clear_pending_exception(env);
        return false;
    }

    let arr_obj: JObject<'_> = arr.into();
    if let Ok(global) = env.new_global_ref(&arr_obj) {
        if let Ok(mut guard) = packed_array_cache().lock() {
            guard.insert(
                handle,
                PackedFloatArray {
                    generation,
                    floats_len: packed_len,
                    array: global,
                },
            );
        }
    }

    call_mesh_emit(env, pose, consumer, &arr_obj, mesh.vertices.len())
}

fn call_mesh_emit(
    env: &mut JNIEnv<'_>,
    pose: &JObject<'_>,
    consumer: &JObject<'_>,
    packed: &JObject<'_>,
    vertex_count: usize,
) -> bool {
    call_mesh_emit_method(env, "emitPacked", pose, consumer, packed, vertex_count)
}

fn call_mesh_emit_method(
    env: &mut JNIEnv<'_>,
    method: &str,
    pose: &JObject<'_>,
    consumer: &JObject<'_>,
    packed: &JObject<'_>,
    vertex_count: usize,
) -> bool {
    
    let class = if let Ok(guard) = mesh_emit_class().lock() {
        if let Some(cls) = guard.as_ref() {
            match env.new_local_ref(cls.as_obj()) {
                Ok(local) => jni::objects::JClass::from(local),
                Err(_) => return false,
            }
        } else {
            return false;
        }
    } else {
        return false;
    };

    if vertex_count > 0x00ff_ffff {
        return false;
    }
    let packed_args = [
        JValue::Object(pose),
        JValue::Object(consumer),
        JValue::Object(packed),
        JValue::Int(vertex_count as i32),
        JValue::Int(0x000A_0000),
        JValue::Int(0x00F0_00F0),
    ];
    let line_args = [
        JValue::Object(pose),
        JValue::Object(consumer),
        JValue::Object(packed),
        JValue::Int(vertex_count as i32),
    ];
    let (descriptor, args) = if method == "emitPacked" {
        (
            "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;[FIII)V",
            packed_args.as_slice(),
        )
    } else {
        (
            "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;[FI)V",
            line_args.as_slice(),
        )
    };
    if env
        .call_static_method(&class, method, descriptor, args)
        .is_err()
        || env.exception_check().unwrap_or(false)
    {
        let detail = jni_bridge::take_pending_exception(env)
            .unwrap_or_else(|| "unknown Java exception".to_string());
        native_jvmti::status::set_error(format!("MeshEmit invoke failed: {detail}"));
        return false;
    }
    true
}

fn resolve_player_uuid(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> Option<Uuid> {
    if !is_avatar_render_state(env, state) {
        return None;
    }
    let (local_id, local_uuid) = local_player_identity(env)?;
    if state_entity_id(env, state)? == local_id {
        return Some(local_uuid);
    }
    None
}

fn client_in_world(env: &mut JNIEnv<'_>) -> bool {
    let Some(minecraft) = minecraft(env) else {
        return false;
    };
    let Ok(level) = env
        .get_field(
            &minecraft,
            "level",
            "Lnet/minecraft/client/multiplayer/ClientLevel;",
        )
        .and_then(|value| value.l())
    else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    if level.is_null() {
        return false;
    }
    let Ok(player) = env
        .get_field(
            &minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )
        .and_then(|value| value.l())
    else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    !player.is_null()
}

fn is_avatar_render_state(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> bool {
    if state.is_null() {
        return false;
    }

    let cached = avatar_render_state_class()
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    if let Some(class) = cached {
        let Ok(local) = env.new_local_ref(class.as_obj()) else {
            jni_bridge::clear_pending_exception(env);
            return false;
        };
        let class = JClass::from(local);
        return env.is_instance_of(state, &class).unwrap_or_else(|_| {
            jni_bridge::clear_pending_exception(env);
            false
        });
    }

    
    
    let Ok(class) = env
        .call_method(state, "getClass", "()Ljava/lang/Class;", &[])
        .and_then(|value| value.l())
    else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    let Ok(name_obj) = env
        .call_method(&class, "getName", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
    else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    let Ok(name) = env.get_string((&name_obj).into()) else {
        jni_bridge::clear_pending_exception(env);
        return false;
    };
    let matches = name.to_string_lossy().contains("AvatarRenderState");
    if matches
        && let Ok(global) = env.new_global_ref(&class)
        && let Ok(mut guard) = avatar_render_state_class().lock()
    {
        *guard = Some(global);
    }
    matches
}

fn is_local_player_state(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> bool {
    if !is_avatar_render_state(env, state) {
        return false;
    }
    let Some((local_id, _)) = local_player_identity(env) else {
        return false;
    };
    state_entity_id(env, state) == Some(local_id)
}

fn state_entity_id(env: &mut JNIEnv<'_>, state: &JObject<'_>) -> Option<i32> {
    match env.get_field(state, "id", "I").and_then(|value| value.i()) {
        Ok(id) => Some(id),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            None
        }
    }
}

fn local_player_identity(env: &mut JNIEnv<'_>) -> Option<(i32, Uuid)> {
    let id = local_player_entity_id(env)?;
    if let Some(cached) = local_player_identity_cache()
        .lock()
        .ok()
        .and_then(|guard| *guard)
        && cached.0 == id
    {
        return Some(cached);
    }

    let uuid = read_local_player_uuid_uncached(env)?;
    let identity = (id, uuid);
    if let Ok(mut guard) = local_player_identity_cache().lock() {
        *guard = Some(identity);
    }
    Some(identity)
}

fn local_player_entity_id(env: &mut JNIEnv<'_>) -> Option<i32> {
    let minecraft = minecraft(env)?;
    let player = env
        .get_field(
            &minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )
        .ok()?
        .l()
        .ok()?;
    if player.is_null() {
        return None;
    }
    env.call_method(&player, "getId", "()I", &[]).ok()?.i().ok()
}

pub fn local_player_uuid(env: &mut JNIEnv<'_>) -> Option<Uuid> {
    local_player_identity(env).map(|(_, uuid)| uuid)
}

fn read_local_player_uuid_uncached(env: &mut JNIEnv<'_>) -> Option<Uuid> {
    let minecraft = minecraft(env)?;
    let player = env
        .get_field(
            &minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )
        .ok()?
        .l()
        .ok()?;
    if player.is_null() {
        return None;
    }
    let uuid = env
        .call_method(&player, "getUUID", "()Ljava/util/UUID;", &[])
        .ok()?
        .l()
        .ok()?;
    let text = env
        .call_method(&uuid, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let java_str = env.get_string((&text).into()).ok()?;
    Uuid::parse_str(&java_str.to_string_lossy()).ok()
}

fn minecraft<'local>(env: &mut JNIEnv<'local>) -> Option<JObject<'local>> {
    let class = jni_bridge::load_class(env, "net.minecraft.client.Minecraft").ok()?;
    env.call_static_method(
        class,
        "getInstance",
        "()Lnet/minecraft/client/Minecraft;",
        &[],
    )
    .ok()?
    .l()
    .ok()
}

pub fn jboolean_from(value: bool) -> jboolean {
    u8::from(value)
}

pub fn jobject_non_null(ptr: jobject) -> bool {
    !ptr.is_null()
}












#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldRenderType {
    
    Lines,
    
    Translucent,
}

impl WorldRenderType {
    fn cache_key(self) -> &'static str {
        match self {
            Self::Lines => "xz:world:lines",
            Self::Translucent => "xz:world:translucent",
        }
    }

    
    
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Self::Lines => &["lines", "debugLineStrip", "linesStrip"],
            Self::Translucent => &["debugFilledBox", "debugQuads", "translucent"],
        }
    }
}







pub fn camera_position(env: &mut JNIEnv<'_>) -> Option<crate::utils::geometry::Vec3d> {
    let minecraft = minecraft(env)?;
    let game_renderer = env
        .get_field(
            &minecraft,
            "gameRenderer",
            "Lnet/minecraft/client/renderer/GameRenderer;",
        )
        .ok()?
        .l()
        .ok()?;
    let camera = env
        .call_method(
            &game_renderer,
            "mainCamera",
            "()Lnet/minecraft/client/Camera;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let position = env
        .get_field(&camera, "position", "Lnet/minecraft/world/phys/Vec3;")
        .ok()?
        .l()
        .ok()?;
    let x = env.get_field(&position, "x", "D").ok()?.d().ok()?;
    let y = env.get_field(&position, "y", "D").ok()?.d().ok()?;
    let z = env.get_field(&position, "z", "D").ok()?.d().ok()?;
    let _ = env.delete_local_ref(position);
    let _ = env.delete_local_ref(camera);
    let _ = env.delete_local_ref(game_renderer);
    Some(crate::utils::geometry::Vec3d { x, y, z })
}


pub fn submit_world_mesh(
    env: &mut JNIEnv<'_>,
    pose_stack: &JObject<'_>,
    collector: &JObject<'_>,
    mesh: Mesh,
    render_type: WorldRenderType,
) -> Result<(), String> {
    if !MESH_EMIT_READY.load(Ordering::Acquire) {
        crate::diag::log_once("world_no_meshemit", "world: MeshEmit class not defined");
        return Err("MeshEmit unavailable".into());
    }
    
    
    let local_stack = pose_stack_copy(env, pose_stack).map_err(|error| {
        crate::diag::log_once("world_pose_copy", format!("world: pose copy failed: {error}"));
        error
    })?;
    let handle = store_mesh(mesh);
    let type_object = world_render_type(env, render_type)?;
    let renderer = geometry_renderer(env, handle)?;
    let result = env.call_method(
        collector,
        RENDER_SYMBOLS.submit_custom_geometry.name,
        RENDER_SYMBOLS.submit_custom_geometry.descriptor,
        &[
            JValue::Object(&local_stack),
            JValue::Object(&type_object),
            JValue::Object(&renderer),
        ],
    );
    if result.is_err() {
        jni_bridge::clear_pending_exception(env);
        return Err("submitCustomGeometry failed for world geometry".into());
    }
    if env.exception_check().unwrap_or(false) {
        jni_bridge::clear_pending_exception(env);
        return Err("world submit raised a Java exception".into());
    }
    Ok(())
}

fn world_render_type<'local>(
    env: &mut JNIEnv<'local>,
    kind: WorldRenderType,
) -> Result<JObject<'local>, String> {
    let key = kind.cache_key();
    if let Ok(guard) = render_type_cache().lock()
        && let Some(global) = guard.get(key)
        && let Ok(local) = env.new_local_ref(global.as_obj())
        && !local.is_null()
    {
        return Ok(local);
    }
    let render_types = env
        .find_class("net/minecraft/client/renderer/rendertype/RenderTypes")
        .map_err(|error| error.to_string())?;
    let mut last_err = String::new();
    for name in kind.candidates() {
        
        match env.call_static_method(
            &render_types,
            name,
            "()Lnet/minecraft/client/renderer/rendertype/RenderType;",
            &[],
        ) {
            Ok(value) => match value.l() {
                Ok(obj) if !obj.is_null() => {
                    if let Ok(global) = env.new_global_ref(&obj)
                        && let Ok(mut guard) = render_type_cache().lock()
                    {
                        guard.insert(key.to_string(), global);
                    }
                    return Ok(obj);
                }
                Ok(_) => last_err = format!("{name} returned null"),
                Err(error) => last_err = error.to_string(),
            },
            Err(error) => {
                jni_bridge::clear_pending_exception(env);
                last_err = error.to_string();
            }
        }
    }
    crate::diag::log_once(
        "world_no_render_type",
        format!("world: no RenderType factory worked ({kind:?}): {last_err}"),
    );
    Err(format!("no world RenderType worked: {last_err}"))
}
