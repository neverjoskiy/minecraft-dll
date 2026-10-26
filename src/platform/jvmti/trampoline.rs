use std::{
    ffi::c_void,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    },
};

use jni::{
    JNIEnv, NativeMethod,
    objects::{GlobalRef, JClass, JObject},
    sys::{
        JNI_FALSE, JNI_TRUE, JNIEnv as RawJniEnv, jboolean, jclass, jdouble, jint, jlong, jobject,
    },
};
use ristretto_classfile::{
    BaseType, ClassAccessFlags, ClassFile, ConstantPool, Field, FieldAccessFlags, FieldType,
    JAVA_6, JAVA_17, Method, MethodAccessFlags,
    attributes::{Attribute, Instruction},
};

use crate::{avatar, jni_bridge};

const HOOKS_INTERNAL: &str = "xz/runtime/NativeHooks4";
const HOOKS_BINARY: &str = "xz.runtime.NativeHooks4";
const GEOMETRY_INTERNAL: &str = "xz/runtime/GeoMeshRenderer";
const GEOMETRY_BINARY: &str = "xz.runtime.GeoMeshRenderer";
const STRING_SUPPLIER_INTERNAL: &str = "xz/runtime/StringSupplier";
const STRING_SUPPLIER_BINARY: &str = "xz.runtime.StringSupplier";
const MESH_EMIT_INTERNAL: &str = "xz/runtime/MeshEmit";
const MESH_EMIT_BINARY: &str = "xz.runtime.MeshEmit";
const MESH_EMIT_BYTES: &[u8] = include_bytes!("../../avatar/render/MeshEmit.class");
const CUSTOM_GEOMETRY_IFACE: &str =
    "net/minecraft/client/renderer/SubmitNodeCollector$CustomGeometryRenderer";

static INSTALLED: AtomicBool = AtomicBool::new(false);
static HEARTBEAT: AtomicU64 = AtomicU64::new(0);
static GEOMETRY_HANDLE: AtomicI64 = AtomicI64::new(0);
static HOOKS_CLASS: OnceLock<GlobalRef> = OnceLock::new();

pub fn install() -> Result<(), String> {
    if INSTALLED.load(Ordering::Acquire) {
        return Ok(());
    }

    let result = jni_bridge::with_env(|env| {
        let hooks = define_or_load(env, HOOKS_BINARY, HOOKS_INTERNAL, build_hooks_class_bytes)?;
        env.register_native_methods(&hooks, &hooks_native_methods())?;
        if HOOKS_CLASS.get().is_none() {
            let _ = HOOKS_CLASS.set(env.new_global_ref(&hooks)?);
        }

        let geometry = define_or_load(
            env,
            GEOMETRY_BINARY,
            GEOMETRY_INTERNAL,
            build_geometry_class_bytes,
        )?;
        env.register_native_methods(&geometry, &geometry_native_methods())?;
        let instance = env.alloc_object(&geometry)?;
        let global = env.new_global_ref(&instance)?;
        avatar::render::store_geometry_renderer(global);

        
        let _ = define_or_load(
            env,
            STRING_SUPPLIER_BINARY,
            STRING_SUPPLIER_INTERNAL,
            build_string_supplier_class_bytes,
        )?;
        
        
        match define_or_load(env, MESH_EMIT_BINARY, MESH_EMIT_INTERNAL, || {
            Ok(MESH_EMIT_BYTES.to_vec())
        }) {
            Ok(mesh_emit) => match env.new_global_ref(&mesh_emit) {
                Ok(global) => avatar::render::store_mesh_emit_class(global),
                Err(error) => {
                    crate::native_jvmti::status::set_error(format!(
                        "MeshEmit global ref failed: {error}"
                    ));
                }
            },
            Err(error) => {
                let detail =
                    jni_bridge::take_pending_exception(env).unwrap_or_else(|| error.to_string());
                crate::native_jvmti::status::set_error(format!("MeshEmit define failed: {detail}"));
            }
        }
        Ok(())
    });

    match result {
        Some(()) => {
            INSTALLED.store(true, Ordering::Release);
            Ok(())
        }
        None => Err("failed to define/register native trampoline classes".to_string()),
    }
}

pub fn shutdown() {
    if !INSTALLED.swap(false, Ordering::AcqRel) {
        return;
    }
    let _ = jni_bridge::with_env(|env| {
        if let Ok(class) = jni_bridge::load_class(env, HOOKS_BINARY) {
            let _ = env.unregister_native_methods(class);
        }
        if let Ok(class) = jni_bridge::load_class(env, GEOMETRY_BINARY) {
            let _ = env.unregister_native_methods(class);
        }
        Ok(())
    });
}

pub fn is_installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}

fn define_or_load<'local, F>(
    env: &mut JNIEnv<'local>,
    binary: &str,
    internal: &str,
    builder: F,
) -> jni::errors::Result<JClass<'local>>
where
    F: FnOnce() -> Result<Vec<u8>, ristretto_classfile::Error>,
{
    match jni_bridge::load_class(env, binary) {
        Ok(class) => Ok(class),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            let loader = jni_bridge::game_class_loader(env).or_else(|_| {
                jni_bridge::clear_pending_exception(env);
                jni_bridge::context_class_loader(env)
            })?;
            let bytes = builder()
                .map_err(|_| jni::errors::Error::JniCall(jni::errors::JniError::Unknown))?;
            env.define_class(internal, &loader, &bytes)
        }
    }
}

fn build_hooks_class_bytes() -> Result<Vec<u8>, ristretto_classfile::Error> {
    let mut constant_pool = ConstantPool::new();
    let this_class = constant_pool.add_class(HOOKS_INTERNAL)?;
    let super_class = constant_pool.add_class("java/lang/Object")?;

    let natives = [
        ("heartbeat", "()J"),
        ("setGeometryHandle", "(J)V"),
        ("getGeometryHandle", "()J"),
        ("onTick", "()V"),
        ("onRender", "(F)V"),
        (
            "onLivingSubmit",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ),
        (
            "onDispatcherSubmit",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;DDDLjava/lang/Object;Ljava/lang/Object;)Z",
        ),
        (
            "onRightHand",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;ILjava/lang/Object;Z)Z",
        ),
        (
            "onLeftHand",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;ILjava/lang/Object;Z)Z",
        ),
        (
            "onAvatarScale",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ),
        (
            "onAvatarBody",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ),
        (
            "onShouldRenderLayers",
            "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ),
        ("onIsBodyVisible", "(Ljava/lang/Object;Ljava/lang/Object;)Z"),
        ("onGeometry", "(Ljava/lang/Object;Ljava/lang/Object;J)V"),
        ("onSubmitWorld", "(Ljava/lang/Object;Ljava/lang/Object;)V"),
        ("onInput", "(III)Z"),
        ("onSound", "(Ljava/lang/Object;Ljava/lang/Object;)Z"),
    ];

    let mut methods = Vec::new();
    for (name, descriptor) in natives {
        methods.push(Method {
            access_flags: MethodAccessFlags::PUBLIC
                | MethodAccessFlags::STATIC
                | MethodAccessFlags::NATIVE,
            name_index: constant_pool.add_utf8(name)?,
            descriptor_index: constant_pool.add_utf8(descriptor)?,
            attributes: Vec::new(),
        });
    }

    let class_file = ClassFile {
        version: JAVA_17,
        constant_pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL | ClassAccessFlags::SUPER,
        this_class,
        super_class,
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods,
        attributes: Vec::new(),
        code_source_url: None,
    };
    class_file.verify()?;
    let mut bytes = Vec::new();
    class_file.to_bytes(&mut bytes)?;
    Ok(bytes)
}

fn build_geometry_class_bytes() -> Result<Vec<u8>, ristretto_classfile::Error> {
    let mut constant_pool = ConstantPool::new();
    let this_class = constant_pool.add_class(GEOMETRY_INTERNAL)?;
    let super_class = constant_pool.add_class("java/lang/Object")?;
    let iface = constant_pool.add_class(CUSTOM_GEOMETRY_IFACE)?;
    let code_name = constant_pool.add_utf8("Code")?;
    let init_name = constant_pool.add_utf8("<init>")?;
    let init_desc = constant_pool.add_utf8("()V")?;
    let object_init = constant_pool.add_method_ref(super_class, "<init>", "()V")?;
    let render_name = constant_pool.add_utf8("render")?;
    let render_desc = constant_pool.add_utf8(
        "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;)V",
    )?;
    let handle_name = constant_pool.add_utf8("handle")?;
    let handle_desc = constant_pool.add_utf8("J")?;

    let fields = vec![Field {
        access_flags: FieldAccessFlags::PUBLIC,
        name_index: handle_name,
        descriptor_index: handle_desc,
        field_type: FieldType::Base(BaseType::Long),
        attributes: Vec::new(),
    }];

    let methods = vec![
        Method {
            access_flags: MethodAccessFlags::PUBLIC,
            name_index: init_name,
            descriptor_index: init_desc,
            attributes: vec![Attribute::Code {
                name_index: code_name,
                max_stack: 1,
                max_locals: 1,
                code: vec![
                    Instruction::Aload_0,
                    Instruction::Invokespecial(object_init),
                    Instruction::Return,
                ],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }],
        },
        Method {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::NATIVE,
            name_index: render_name,
            descriptor_index: render_desc,
            attributes: Vec::new(),
        },
    ];

    let class_file = ClassFile {
        version: JAVA_17,
        constant_pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL | ClassAccessFlags::SUPER,
        this_class,
        super_class,
        interfaces: vec![iface],
        fields,
        methods,
        attributes: Vec::new(),
        code_source_url: None,
    };
    class_file.verify()?;
    let mut bytes = Vec::new();
    class_file.to_bytes(&mut bytes)?;
    Ok(bytes)
}


fn build_string_supplier_class_bytes() -> Result<Vec<u8>, ristretto_classfile::Error> {
    let mut constant_pool = ConstantPool::new();
    let this_class = constant_pool.add_class(STRING_SUPPLIER_INTERNAL)?;
    let super_class = constant_pool.add_class("java/lang/Object")?;
    let supplier_iface = constant_pool.add_class("java/util/function/Supplier")?;
    let code_name = constant_pool.add_utf8("Code")?;
    let value_name = constant_pool.add_utf8("value")?;
    let value_desc = constant_pool.add_utf8("Ljava/lang/String;")?;
    let init_name = constant_pool.add_utf8("<init>")?;
    let init_desc = constant_pool.add_utf8("(Ljava/lang/String;)V")?;
    let get_name = constant_pool.add_utf8("get")?;
    let get_desc = constant_pool.add_utf8("()Ljava/lang/Object;")?;
    let object_init = constant_pool.add_method_ref(super_class, "<init>", "()V")?;
    let value_field = constant_pool.add_field_ref(this_class, "value", "Ljava/lang/String;")?;

    let fields = vec![Field {
        access_flags: FieldAccessFlags::PRIVATE | FieldAccessFlags::FINAL,
        name_index: value_name,
        descriptor_index: value_desc,
        field_type: FieldType::Object("java/lang/String".into()),
        attributes: Vec::new(),
    }];

    let methods = vec![
        Method {
            access_flags: MethodAccessFlags::PUBLIC,
            name_index: init_name,
            descriptor_index: init_desc,
            attributes: vec![Attribute::Code {
                name_index: code_name,
                max_stack: 2,
                max_locals: 2,
                code: vec![
                    Instruction::Aload_0,
                    Instruction::Invokespecial(object_init),
                    Instruction::Aload_0,
                    Instruction::Aload_1,
                    Instruction::Putfield(value_field),
                    Instruction::Return,
                ],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }],
        },
        Method {
            access_flags: MethodAccessFlags::PUBLIC,
            name_index: get_name,
            descriptor_index: get_desc,
            attributes: vec![Attribute::Code {
                name_index: code_name,
                max_stack: 1,
                max_locals: 1,
                code: vec![
                    Instruction::Aload_0,
                    Instruction::Getfield(value_field),
                    Instruction::Areturn,
                ],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }],
        },
    ];

    let class_file = ClassFile {
        version: JAVA_17,
        constant_pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL | ClassAccessFlags::SUPER,
        this_class,
        super_class,
        interfaces: vec![supplier_iface],
        fields,
        methods,
        attributes: Vec::new(),
        code_source_url: None,
    };
    class_file.verify()?;
    let mut bytes = Vec::new();
    class_file.to_bytes(&mut bytes)?;
    Ok(bytes)
}

fn hooks_native_methods() -> Vec<NativeMethod> {
    vec![
        NativeMethod {
            name: "heartbeat".into(),
            sig: "()J".into(),
            fn_ptr: native_heartbeat as *mut c_void,
        },
        NativeMethod {
            name: "setGeometryHandle".into(),
            sig: "(J)V".into(),
            fn_ptr: native_set_geometry_handle as *mut c_void,
        },
        NativeMethod {
            name: "getGeometryHandle".into(),
            sig: "()J".into(),
            fn_ptr: native_get_geometry_handle as *mut c_void,
        },
        NativeMethod {
            name: "onTick".into(),
            sig: "()V".into(),
            fn_ptr: native_on_tick as *mut c_void,
        },
        NativeMethod {
            name: "onRender".into(),
            sig: "(F)V".into(),
            fn_ptr: native_on_render as *mut c_void,
        },
        NativeMethod {
            name: "onLivingSubmit".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z"
                .into(),
            fn_ptr: native_on_living_submit as *mut c_void,
        },
        NativeMethod {
            name: "onDispatcherSubmit".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;DDDLjava/lang/Object;Ljava/lang/Object;)Z"
                .into(),
            fn_ptr: native_on_dispatcher_submit as *mut c_void,
        },
        NativeMethod {
            name: "onRightHand".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;ILjava/lang/Object;Z)Z"
                .into(),
            fn_ptr: native_on_right_hand as *mut c_void,
        },
        NativeMethod {
            name: "onLeftHand".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;ILjava/lang/Object;Z)Z"
                .into(),
            fn_ptr: native_on_left_hand as *mut c_void,
        },
        NativeMethod {
            name: "onAvatarScale".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
            fn_ptr: native_on_avatar_scale as *mut c_void,
        },
        NativeMethod {
            name: "onAvatarBody".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z"
                .into(),
            fn_ptr: native_on_avatar_body as *mut c_void,
        },
        NativeMethod {
            name: "onShouldRenderLayers".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
            fn_ptr: native_on_should_render_layers as *mut c_void,
        },
        NativeMethod {
            name: "onIsBodyVisible".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
            fn_ptr: native_on_is_body_visible as *mut c_void,
        },
        NativeMethod {
            name: "onGeometry".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;J)V".into(),
            fn_ptr: native_on_geometry as *mut c_void,
        },
        NativeMethod {
            name: "onSubmitWorld".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;)V".into(),
            fn_ptr: native_on_submit_world as *mut c_void,
        },
        NativeMethod {
            name: "onInput".into(),
            sig: "(III)Z".into(),
            fn_ptr: native_on_input as *mut c_void,
        },
        NativeMethod {
            name: "onSound".into(),
            sig: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
            fn_ptr: native_on_sound as *mut c_void,
        },
    ]
}

fn geometry_native_methods() -> Vec<NativeMethod> {
    vec![NativeMethod {
        name: "render".into(),
        sig: "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;)V"
            .into(),
        fn_ptr: native_geometry_render as *mut c_void,
    }]
}

fn with_env_from_raw<R>(raw: *mut RawJniEnv, f: impl FnOnce(&mut JNIEnv<'_>) -> R) -> Option<R> {
    if raw.is_null() {
        return None;
    }
    let mut env = unsafe { JNIEnv::from_raw(raw).ok()? };
    
    env.with_local_frame(128, |env| Ok::<R, jni::errors::Error>(f(env)))
        .ok()
}

unsafe extern "system" fn native_heartbeat(_env: *mut RawJniEnv, _class: jclass) -> jlong {
    HEARTBEAT.fetch_add(1, Ordering::Relaxed).wrapping_add(1) as jlong
}

unsafe extern "system" fn native_set_geometry_handle(
    _env: *mut RawJniEnv,
    _class: jclass,
    handle: jlong,
) {
    GEOMETRY_HANDLE.store(handle, Ordering::Release);
}

unsafe extern "system" fn native_get_geometry_handle(
    _env: *mut RawJniEnv,
    _class: jclass,
) -> jlong {
    GEOMETRY_HANDLE.load(Ordering::Acquire)
}

unsafe extern "system" fn native_on_tick(_env: *mut RawJniEnv, _class: jclass) {}

unsafe extern "system" fn native_on_render(
    _env: *mut RawJniEnv,
    _class: jclass,
    _delta: jni::sys::jfloat,
) {
}

unsafe extern "system" fn native_on_living_submit(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    state: jobject,
    pose_stack: jobject,
    collector: jobject,
    camera: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let state = unsafe { JObject::from_raw(state) };
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let collector = unsafe { JObject::from_raw(collector) };
        let camera = unsafe { JObject::from_raw(camera) };
        let replaced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_living_submit(
                env,
                &renderer,
                &state,
                &pose_stack,
                &collector,
                &camera,
            )
        }))
        .unwrap_or(false);
        if replaced { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_FALSE)
}

unsafe extern "system" fn native_on_dispatcher_submit(
    env: *mut RawJniEnv,
    _class: jclass,
    dispatcher: jobject,
    state: jobject,
    camera: jobject,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    pose_stack: jobject,
    collector: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let dispatcher = unsafe { JObject::from_raw(dispatcher) };
        let state = unsafe { JObject::from_raw(state) };
        let camera = unsafe { JObject::from_raw(camera) };
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let collector = unsafe { JObject::from_raw(collector) };
        let replaced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_dispatcher_submit(
                env,
                &dispatcher,
                &state,
                &camera,
                x,
                y,
                z,
                &pose_stack,
                &collector,
            )
        }))
        .unwrap_or(false);
        if replaced { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_FALSE)
}

unsafe extern "system" fn native_on_right_hand(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    pose_stack: jobject,
    collector: jobject,
    light: jint,
    _texture: jobject,
    _sleeve: jboolean,
) -> jboolean {
    hand_hook(env, renderer, pose_stack, collector, light, true)
}

unsafe extern "system" fn native_on_left_hand(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    pose_stack: jobject,
    collector: jobject,
    light: jint,
    _texture: jobject,
    _sleeve: jboolean,
) -> jboolean {
    hand_hook(env, renderer, pose_stack, collector, light, false)
}

fn hand_hook(
    env: *mut RawJniEnv,
    renderer: jobject,
    pose_stack: jobject,
    collector: jobject,
    light: jint,
    right: bool,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let collector = unsafe { JObject::from_raw(collector) };
        let replaced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_first_person_hand(
                env,
                &renderer,
                &pose_stack,
                &collector,
                light,
                right,
            )
        }))
        .unwrap_or(false);
        jni_bridge::clear_pending_exception(env);
        if replaced { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_FALSE)
}

unsafe extern "system" fn native_on_avatar_scale(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    state: jobject,
    pose_stack: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let state = unsafe { JObject::from_raw(state) };
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let replaced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_avatar_scale(env, &renderer, &state, &pose_stack)
        }))
        .unwrap_or(false);
        if replaced { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_FALSE)
}

unsafe extern "system" fn native_on_avatar_body(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    state: jobject,
    pose_stack: jobject,
    collector: jobject,
    camera: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let state = unsafe { JObject::from_raw(state) };
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let collector = unsafe { JObject::from_raw(collector) };
        let camera = unsafe { JObject::from_raw(camera) };
        let replaced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_avatar_body(env, &renderer, &state, &pose_stack, &collector, &camera)
        }))
        .unwrap_or(false);
        if replaced { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_FALSE)
}

unsafe extern "system" fn native_on_should_render_layers(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    state: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let state = unsafe { JObject::from_raw(state) };
        let show = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_should_render_layers(env, &renderer, &state)
        }))
        .unwrap_or(true);
        if show { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_TRUE)
}

unsafe extern "system" fn native_on_is_body_visible(
    env: *mut RawJniEnv,
    _class: jclass,
    renderer: jobject,
    state: jobject,
) -> jboolean {
    with_env_from_raw(env, |env| {
        let renderer = unsafe { JObject::from_raw(renderer) };
        let state = unsafe { JObject::from_raw(state) };
        let visible = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_is_body_visible(env, &renderer, &state)
        }))
        .unwrap_or(true);
        if visible { JNI_TRUE } else { JNI_FALSE }
    })
    .unwrap_or(JNI_TRUE)
}

unsafe extern "system" fn native_on_submit_world(
    env: *mut RawJniEnv,
    _class: jclass,
    pose_stack: jobject,
    collector: jobject,
) {
    let _ = with_env_from_raw(env, |env| {
        let pose_stack = unsafe { JObject::from_raw(pose_stack) };
        let collector = unsafe { JObject::from_raw(collector) };
        
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::world_render::on_submit_world(env, &pose_stack, &collector);
        }));
    });
}

unsafe extern "system" fn native_on_geometry(
    env: *mut RawJniEnv,
    _class: jclass,
    pose: jobject,
    consumer: jobject,
    handle: jlong,
) {
    let _ = with_env_from_raw(env, |env| {
        let pose = unsafe { JObject::from_raw(pose) };
        let consumer = unsafe { JObject::from_raw(consumer) };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_geometry(env, &pose, &consumer, handle);
        }));
    });
}

unsafe extern "system" fn native_geometry_render(
    env: *mut RawJniEnv,
    this: jobject,
    pose: jobject,
    consumer: jobject,
) {
    let _ = with_env_from_raw(env, |jni_env| {
        let this_obj = unsafe { JObject::from_raw(this) };
        let handle = jni_env
            .get_field(&this_obj, "handle", "J")
            .ok()
            .and_then(|value| value.j().ok())
            .unwrap_or_else(|| GEOMETRY_HANDLE.load(Ordering::Acquire));
        let pose = unsafe { JObject::from_raw(pose) };
        let consumer = unsafe { JObject::from_raw(consumer) };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            avatar::render::on_geometry(jni_env, &pose, &consumer, handle);
        }));
        Ok::<(), ()>(())
    });
}

unsafe extern "system" fn native_on_input(
    _env: *mut RawJniEnv,
    _class: jclass,
    _key: jint,
    _action: jint,
    _modifiers: jint,
) -> jboolean {
    JNI_FALSE
}

unsafe extern "system" fn native_on_sound(
    _env: *mut RawJniEnv,
    _class: jclass,
    _sound: jobject,
    _source: jobject,
) -> jboolean {
    JNI_FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_hooks_and_geometry_classes_round_trip() {
        let hooks = build_hooks_class_bytes().expect("hooks");
        let geometry = build_geometry_class_bytes().expect("geometry");
        let mesh_emit = MESH_EMIT_BYTES;
        let hooks = ClassFile::from_bytes(&hooks).expect("parse hooks");
        let geometry = ClassFile::from_bytes(&geometry).expect("parse geometry");
        let mesh_emit = ClassFile::from_bytes(&mesh_emit).expect("parse mesh emit");
        assert_eq!(hooks.class_name().unwrap().to_string(), HOOKS_INTERNAL);
        assert_eq!(
            geometry.class_name().unwrap().to_string(),
            GEOMETRY_INTERNAL
        );
        assert_eq!(geometry.interfaces.len(), 1);
        assert_eq!(
            mesh_emit.class_name().unwrap().to_string(),
            MESH_EMIT_INTERNAL
        );
        assert!(mesh_emit.methods.iter().any(|method| {
            mesh_emit
                .constant_pool
                .try_get_utf8(method.name_index)
                .is_ok_and(|name| name == "emitPacked")
                && mesh_emit
                    .constant_pool
                    .try_get_utf8(method.descriptor_index)
                    .is_ok_and(|descriptor| {
                        descriptor
                            == "(Lcom/mojang/blaze3d/vertex/PoseStack$Pose;Lcom/mojang/blaze3d/vertex/VertexConsumer;[FIII)V"
                    })
        }));
        assert!(mesh_emit.methods.iter().any(|method| {
            mesh_emit
                .constant_pool
                .try_get_utf8(method.name_index)
                .is_ok_and(|name| name == "emitLines")
        }));
    }

    
    
    
    
    
    
    
    
    
    
    #[test]
    fn mesh_emit_transforms_every_vertex_by_the_pose() {
        let mesh_emit = ClassFile::from_bytes(&MESH_EMIT_BYTES).expect("parse mesh emit");
        let pool = &mesh_emit.constant_pool;

        
        let invoked = |index: u16| -> Option<String> {
            let (_, name_and_type) = pool
                .try_get_interface_method_ref(index)
                .or_else(|_| pool.try_get_method_ref(index))
                .ok()?;
            let (name_index, descriptor_index) = pool.try_get_name_and_type(*name_and_type).ok()?;
            let name = pool.try_get_utf8(*name_index).ok()?;
            let descriptor = pool.try_get_utf8(*descriptor_index).ok()?;
            Some(format!("{name}:{descriptor}"))
        };

        const POSE: &str = "Lcom/mojang/blaze3d/vertex/PoseStack$Pose;";
        let mut checked_methods = 0;

        for method in &mesh_emit.methods {
            let name = pool
                .try_get_utf8(method.name_index)
                .expect("method name")
                .to_string();
            if name != "emitPacked" && name != "emitLines" {
                continue;
            }
            let code = method
                .attributes
                .iter()
                .find_map(|attribute| match attribute {
                    Attribute::Code { code, .. } => Some(code),
                    _ => None,
                })
                .expect("emitter has a Code attribute");

            let calls: Vec<String> = code
                .iter()
                .filter_map(|instruction| match instruction {
                    Instruction::Invokeinterface(index, _) => invoked(*index),
                    Instruction::Invokevirtual(index) | Instruction::Invokestatic(index) => {
                        invoked(*index)
                    }
                    _ => None,
                })
                .collect();

            
            let pose_vertices = calls
                .iter()
                .filter(|call| call.starts_with(&format!("addVertex:({POSE}FFF)")))
                .count();
            assert!(
                pose_vertices > 0,
                "{name} must call addVertex(Pose, x, y, z); calls were {calls:?}"
            );

            
            for call in &calls {
                assert!(
                    !call.starts_with("addVertex:(FFF"),
                    "{name} calls pose-less {call}, which discards the PoseStack transform \
                     and renders the avatar at camera origin (huge / off-screen)"
                );
            }

            
            assert!(
                code.iter().any(|i| matches!(i, Instruction::Aload_0)),
                "{name} never loads its Pose parameter (slot 0)"
            );

            checked_methods += 1;
        }

        assert_eq!(
            checked_methods, 2,
            "expected to verify both emitPacked and emitLines"
        );
    }
}
