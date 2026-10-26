use std::{mem::transmute, ptr::null_mut, sync::OnceLock};

use jni::{
    JNIEnv, JavaVM,
    errors::Result as JniResult,
    objects::{JClass, JObject, JValue},
    sys::{JNI_FALSE, JNI_OK, JavaVM as RawJavaVM, jint, jsize},
};
use windows::{
    Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    core::s,
};

type JniGetCreatedJavaVMs =
    unsafe extern "system" fn(*mut *mut RawJavaVM, jsize, *mut jsize) -> jint;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

pub fn with_env<R>(f: impl FnOnce(&mut JNIEnv<'_>) -> JniResult<R>) -> Option<R> {
    let java_vm = java_vm()?;
    let mut env = java_vm.attach_current_thread().ok()?;
    env.with_local_frame(512, f).ok()
}




pub fn with_attached<R>(f: impl FnOnce(&mut JNIEnv<'_>) -> JniResult<R>) -> Option<R> {
    let java_vm = java_vm()?;
    let mut env = java_vm.attach_current_thread().ok()?;
    let result = f(&mut env);
    if result.is_err() {
        clear_pending_exception(&mut env);
    }
    result.ok()
}

pub fn is_game_active() -> bool {
    with_env(is_game_active_env).unwrap_or(false)
}

pub fn is_world_active() -> bool {
    with_env(is_world_active_env).unwrap_or(false)
}

pub fn release_mouse_for_overlay() -> bool {
    with_env(release_mouse_for_overlay_env).unwrap_or(false)
}

pub fn grab_mouse_after_overlay() {
    let _ = with_env(grab_mouse_after_overlay_env);
}

pub fn context_class_loader<'local>(env: &mut JNIEnv<'local>) -> JniResult<JObject<'local>> {
    let thread = env
        .call_static_method(
            "java/lang/Thread",
            "currentThread",
            "()Ljava/lang/Thread;",
            &[],
        )?
        .l()?;
    env.call_method(
        &thread,
        "getContextClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    )?
    .l()
}

pub fn load_class<'local>(env: &mut JNIEnv<'local>, name: &str) -> JniResult<JClass<'local>> {
    let class_loader = context_class_loader(env)?;
    load_class_with_loader(env, &class_loader, name)
}




pub fn load_game_class<'local>(env: &mut JNIEnv<'local>, name: &str) -> JniResult<JClass<'local>> {
    if let Ok(minecraft) = minecraft_instance(env)
        && !minecraft.is_null()
    {
        let mc_class = env.get_object_class(&minecraft)?;
        let loader = env
            .call_method(mc_class, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        if !loader.is_null() {
            match load_class_with_loader(env, &loader, name) {
                Ok(class) => return Ok(class),
                Err(_) => clear_pending_exception(env),
            }
        }
    } else {
        clear_pending_exception(env);
    }
    load_class(env, name)
}

pub fn game_class_loader<'local>(env: &mut JNIEnv<'local>) -> JniResult<JObject<'local>> {
    let minecraft = minecraft_instance(env)?;
    if minecraft.is_null() {
        return Err(jni::errors::Error::NullPtr("Minecraft instance"));
    }
    let minecraft_class = env.get_object_class(&minecraft)?;
    env.call_method(
        minecraft_class,
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    )?
    .l()
}

fn load_class_with_loader<'local>(
    env: &mut JNIEnv<'local>,
    loader: &JObject<'local>,
    name: &str,
) -> JniResult<JClass<'local>> {
    let class_name = env.new_string(name)?;
    let class_name = JObject::from(class_name);
    let class = env
        .call_method(
            loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&class_name)],
        )?
        .l()?;
    Ok(JClass::from(class))
}

pub fn clear_pending_exception(env: &mut JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

pub fn take_pending_exception(env: &mut JNIEnv<'_>) -> Option<String> {
    if !env.exception_check().ok()? {
        return None;
    }
    let throwable = env.exception_occurred().ok()?;
    let _ = env.exception_clear();
    let text = env
        .call_method(throwable, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    if text.is_null() {
        return None;
    }
    env.get_string((&text).into())
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

pub fn keep_afk_alive() {
    let _ = with_env(|env| {
        let minecraft = minecraft_instance(env)?;
        let tracker = env
            .call_method(
                &minecraft,
                "getFramerateLimitTracker",
                "()Lcom/mojang/blaze3d/platform/FramerateLimitTracker;",
                &[],
            )?
            .l()?;
        env.call_method(&tracker, "onInputReceived", "()V", &[])?;
        Ok(())
    });
}

pub(crate) fn java_vm() -> Option<&'static JavaVM> {
    if JAVA_VM.get().is_none() {
        let java_vm = discover_java_vm()?;
        let _ = JAVA_VM.set(java_vm);
    }

    JAVA_VM.get()
}

fn discover_java_vm() -> Option<JavaVM> {
    let jvm = unsafe { GetModuleHandleA(s!("jvm.dll")).ok()? };
    let proc = unsafe { GetProcAddress(jvm, s!("JNI_GetCreatedJavaVMs"))? };
    let get_created_java_vms: JniGetCreatedJavaVMs = unsafe { transmute(proc) };

    let mut vm = null_mut();
    let mut count = 0;
    let result = unsafe { get_created_java_vms(&mut vm, 1, &mut count) };
    if result != JNI_OK || count <= 0 || vm.is_null() {
        return None;
    }

    unsafe { JavaVM::from_raw(vm).ok() }
}

fn is_game_active_env(env: &mut JNIEnv<'_>) -> JniResult<bool> {
    let result = (|| {
        let minecraft = minecraft_instance(env)?;
        if !is_world_active_for_minecraft(env, &minecraft)? {
            return Ok(false);
        }

        let screen = minecraft_screen(env, &minecraft)?;
        Ok(screen.is_null())
    })();

    if result.is_err() {
        clear_pending_exception(env);
    }

    result
}

fn is_world_active_env(env: &mut JNIEnv<'_>) -> JniResult<bool> {
    let result = (|| {
        let minecraft = minecraft_instance(env)?;
        is_world_active_for_minecraft(env, &minecraft)
    })();

    if result.is_err() {
        clear_pending_exception(env);
    }

    result
}

fn is_world_active_for_minecraft(env: &mut JNIEnv<'_>, minecraft: &JObject<'_>) -> JniResult<bool> {
    if minecraft.is_null() {
        return Ok(false);
    }

    let level = env
        .get_field(
            minecraft,
            "level",
            "Lnet/minecraft/client/multiplayer/ClientLevel;",
        )?
        .l()?;
    if level.is_null() {
        return Ok(false);
    }

    let player = env
        .get_field(
            minecraft,
            "player",
            "Lnet/minecraft/client/player/LocalPlayer;",
        )?
        .l()?;
    if player.is_null() {
        return Ok(false);
    }

    let overlay = minecraft_overlay(env, minecraft)?;
    Ok(overlay.is_null())
}


pub fn minecraft_gui<'local>(
    env: &mut JNIEnv<'local>,
    minecraft: &JObject<'_>,
) -> JniResult<JObject<'local>> {
    env.get_field(minecraft, "gui", "Lnet/minecraft/client/gui/Gui;")?
        .l()
}

pub fn minecraft_screen<'local>(
    env: &mut JNIEnv<'local>,
    minecraft: &JObject<'_>,
) -> JniResult<JObject<'local>> {
    let gui = minecraft_gui(env, minecraft)?;
    if gui.is_null() {
        return Ok(JObject::null());
    }

    let screen = env
        .call_method(
            &gui,
            "screen",
            "()Lnet/minecraft/client/gui/screens/Screen;",
            &[],
        )?
        .l()?;
    let _ = env.delete_local_ref(gui);
    Ok(screen)
}

pub fn minecraft_overlay<'local>(
    env: &mut JNIEnv<'local>,
    minecraft: &JObject<'_>,
) -> JniResult<JObject<'local>> {
    let gui = minecraft_gui(env, minecraft)?;
    if gui.is_null() {
        return Ok(JObject::null());
    }

    let overlay = env
        .call_method(
            &gui,
            "overlay",
            "()Lnet/minecraft/client/gui/screens/Overlay;",
            &[],
        )?
        .l()?;
    let _ = env.delete_local_ref(gui);
    Ok(overlay)
}

fn release_mouse_for_overlay_env(env: &mut JNIEnv<'_>) -> JniResult<bool> {
    let result = (|| {
        let minecraft = minecraft_instance(env)?;
        if minecraft.is_null() {
            return Ok(false);
        }

        let mouse_handler = env
            .get_field(
                &minecraft,
                "mouseHandler",
                "Lnet/minecraft/client/MouseHandler;",
            )?
            .l()?;
        if mouse_handler.is_null() {
            return Ok(false);
        }

        let was_grabbed = env
            .call_method(&mouse_handler, "isMouseGrabbed", "()Z", &[])?
            .z()?;
        force_cursor_normal(env, &minecraft, &mouse_handler)?;
        release_key_mappings(env)?;

        Ok(was_grabbed)
    })();

    if result.is_err() {
        clear_pending_exception(env);
    }

    result
}

fn grab_mouse_after_overlay_env(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let result = (|| {
        if !is_game_active_env(env)? {
            return Ok(());
        }

        let minecraft = minecraft_instance(env)?;
        if minecraft.is_null() {
            return Ok(());
        }

        let mouse_handler = env
            .get_field(
                &minecraft,
                "mouseHandler",
                "Lnet/minecraft/client/MouseHandler;",
            )?
            .l()?;
        if !mouse_handler.is_null() {
            env.call_method(&mouse_handler, "grabMouse", "()V", &[])?;
        }

        Ok(())
    })();

    if result.is_err() {
        clear_pending_exception(env);
    }

    result
}

fn minecraft_instance<'local>(env: &mut JNIEnv<'local>) -> JniResult<JObject<'local>> {
    let minecraft_class = load_class(env, "net.minecraft.client.Minecraft")?;
    env.call_static_method(
        minecraft_class,
        "getInstance",
        "()Lnet/minecraft/client/Minecraft;",
        &[],
    )?
    .l()
}

fn force_cursor_normal(
    env: &mut JNIEnv<'_>,
    minecraft: &JObject<'_>,
    mouse_handler: &JObject<'_>,
) -> JniResult<()> {
    env.set_field(mouse_handler, "mouseGrabbed", "Z", JValue::Bool(JNI_FALSE))?;
    env.call_method(mouse_handler, "setIgnoreFirstMove", "()V", &[])?;

    let window = env
        .call_method(
            minecraft,
            "getWindow",
            "()Lcom/mojang/blaze3d/platform/Window;",
            &[],
        )?
        .l()?;
    if window.is_null() {
        return Ok(());
    }

    let window_handle = env.call_method(&window, "handle", "()J", &[])?.j()?;
    let glfw = load_class(env, "org.lwjgl.glfw.GLFW")?;
    env.call_static_method(
        glfw,
        "glfwSetInputMode",
        "(JII)V",
        &[
            JValue::Long(window_handle),
            JValue::Int(208897),
            JValue::Int(212993),
        ],
    )?;

    Ok(())
}

fn release_key_mappings(env: &mut JNIEnv<'_>) -> JniResult<()> {
    let key_mapping = load_class(env, "net.minecraft.client.KeyMapping")?;
    env.call_static_method(key_mapping, "releaseAll", "()V", &[])?;
    Ok(())
}
