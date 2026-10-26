mod profiles;
pub mod status;
mod trampoline;
mod transformer;

use std::{
    ffi::{CStr, c_char, c_uchar},
    ptr,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicBool, AtomicPtr, Ordering},
    },
};

use jvmti_bindings::{
    Agent,
    env::Jvmti,
    get_default_callbacks, set_global_agent,
    sys::{jni as raw_jni, jvmti as raw_jvmti},
};

pub use profiles::{
    GAME_VERSION, MethodSymbol, ProfileValidation, RENDER_SYMBOLS, RenderSymbols, RuntimeLoader,
    RuntimeProfile,
};
pub use status::JvmtiStatus;
pub use transformer::{HookSpec, LocalLoad, clear_hooks, hooked_class_names};

use crate::jni_bridge;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static EVENTS_ON: AtomicBool = AtomicBool::new(false);
static ENV: AtomicPtr<raw_jvmti::jvmtiEnv> = AtomicPtr::new(ptr::null_mut());

fn profile_state() -> &'static RwLock<Option<RuntimeProfile>> {
    static PROFILE: OnceLock<RwLock<Option<RuntimeProfile>>> = OnceLock::new();
    PROFILE.get_or_init(|| RwLock::new(None))
}

#[derive(Default)]
struct NativeAgent;

impl Agent for NativeAgent {
    fn on_load(&self, _vm: *mut raw_jni::JavaVM, _options: &str) -> raw_jni::jint {
        raw_jni::JNI_OK
    }

    #[allow(clippy::too_many_arguments)]
    fn class_file_load_hook_with_jvmti(
        &self,
        jvmti_env: *mut raw_jvmti::jvmtiEnv,
        _jni: *mut raw_jni::JNIEnv,
        _class_being_redefined: raw_jni::jclass,
        _loader: raw_jni::jobject,
        name: *const c_char,
        _protection_domain: raw_jni::jobject,
        class_data_len: raw_jni::jint,
        class_data: *const c_uchar,
        new_class_data_len: *mut raw_jni::jint,
        new_class_data: *mut *mut c_uchar,
    ) {
        if !ACTIVE.load(Ordering::Acquire)
            || !EVENTS_ON.load(Ordering::Acquire)
            || name.is_null()
            || class_data.is_null()
            || class_data_len <= 0
            || new_class_data_len.is_null()
            || new_class_data.is_null()
        {
            return;
        }

        let Ok(class_name) = (unsafe { CStr::from_ptr(name) }).to_str() else {
            return;
        };
        if !transformer::has_hook_for(class_name) {
            return;
        }

        let input = unsafe { std::slice::from_raw_parts(class_data, class_data_len as usize) };
        crate::diag::log(format!("cflh: {class_name} in={} bytes", input.len()));
        
        
        crate::diag::dump_class_once(class_name, input);
        if let Err(error) = profiles::validate_target_class(class_name, input) {
            crate::diag::log(format!("cflh: {class_name} PROFILE REJECT {error}"));
            status::set_error(format!("profile rejected {class_name}: {error}"));
            return;
        }
        let transformed = match transformer::transform(class_name, input) {
            Ok(Some(bytes)) => {
                crate::diag::log(format!(
                    "cflh: {class_name} transformed out={} bytes",
                    bytes.len()
                ));
                bytes
            }
            Ok(None) => {
                crate::diag::log(format!("cflh: {class_name} no-op (already hooked?)"));
                return;
            }
            Err(error) => {
                crate::diag::log(format!("cflh: {class_name} TRANSFORM ERROR {error}"));
                status::set_error(format!("transform {class_name}: {error}"));
                return;
            }
        };

        let jvmti = unsafe { Jvmti::from_raw(jvmti_env) };
        let Ok(output) = jvmti.allocate(transformed.len() as raw_jni::jlong) else {
            status::set_error(format!("JVMTI allocate failed for {class_name}"));
            return;
        };
        unsafe {
            ptr::copy_nonoverlapping(transformed.as_ptr(), output, transformed.len());
            *new_class_data_len = transformed.len() as raw_jni::jint;
            *new_class_data = output;
        }
        status::update(|current| {
            current.transformed_classes = current.transformed_classes.saturating_add(1);
            current.last_error.clear();
        });
    }
}



pub fn install() -> Result<(), String> {
    if ACTIVE.load(Ordering::Acquire) {
        return Ok(());
    }

    let java_vm = jni_bridge::java_vm().ok_or("Java VM was not discovered")?;
    let raw_vm = java_vm.get_java_vm_pointer() as *mut raw_jni::JavaVM;
    let jvmti =
        Jvmti::new(raw_vm).map_err(|code| format!("GetEnv(JVMTI_VERSION_1_2) failed: {code}"))?;

    set_global_agent(Box::<NativeAgent>::default())
        .map_err(|_| "JVMTI agent callback router is already initialized".to_string())?;

    
    
    let mut capabilities = raw_jvmti::jvmtiCapabilities::default();
    capabilities.set_can_retransform_classes(true);
    capabilities.set_can_retransform_any_class(true);
    capabilities.set_can_redefine_classes(true);
    capabilities.set_can_redefine_any_class(true);
    capabilities.set_can_generate_all_class_hook_events(true);
    jvmti
        .add_capabilities(&capabilities)
        .map_err(|error| format!("AddCapabilities failed: {error:?}"))?;
    jvmti
        .set_event_callbacks(get_default_callbacks())
        .map_err(|error| format!("SetEventCallbacks failed: {error:?}"))?;

    ENV.store(jvmti.raw(), Ordering::Release);
    ACTIVE.store(true, Ordering::Release);
    EVENTS_ON.store(false, Ordering::Release);

    let profile = RuntimeProfile::detect(&jvmti);
    if !profile.is_supported() {
        let error = format!(
            "unsupported Minecraft runtime {} (protocol {})",
            profile.game_version, profile.protocol_version
        );
        status::set_error(error.clone());
        
    }
    if let Ok(mut current) = profile_state().write() {
        *current = Some(profile.clone());
    }
    status::update(|current| {
        current.installed = true;
        current.events_enabled = false;
        current.trampoline_defined = false;
        current.detected_runtime = profile.id();
        current.last_error.clear();
    });
    Ok(())
}


pub fn install_trampoline() -> Result<(), String> {
    if !ACTIVE.load(Ordering::Acquire) {
        install()?;
    }
    if trampoline::is_installed() {
        status::update(|current| current.trampoline_defined = true);
        return Ok(());
    }
    trampoline::install()?;
    status::update(|current| {
        current.trampoline_defined = true;
        current.last_error.clear();
    });
    Ok(())
}


pub fn enable_class_file_hook() -> Result<(), String> {
    let env = ENV.load(Ordering::Acquire);
    if env.is_null() || !ACTIVE.load(Ordering::Acquire) {
        return Err("JVMTI is not installed".to_string());
    }
    if EVENTS_ON.load(Ordering::Acquire) {
        return Ok(());
    }

    let jvmti = unsafe { Jvmti::from_raw(env) };
    jvmti
        .enable_events_global(&[raw_jvmti::JVMTI_EVENT_CLASS_FILE_LOAD_HOOK])
        .map_err(|error| format!("enabling ClassFileLoadHook failed: {error:?}"))?;
    EVENTS_ON.store(true, Ordering::Release);
    status::update(|current| {
        current.events_enabled = true;
        current.last_error.clear();
    });
    Ok(())
}

pub fn shutdown() {
    if !ACTIVE.swap(false, Ordering::AcqRel) {
        return;
    }

    EVENTS_ON.store(false, Ordering::Release);
    let env = ENV.swap(ptr::null_mut(), Ordering::AcqRel);
    if !env.is_null() {
        let jvmti = unsafe { Jvmti::from_raw(env) };
        let _ = jvmti.disable_events_global(&[
            raw_jvmti::JVMTI_EVENT_CLASS_FILE_LOAD_HOOK,
            raw_jvmti::JVMTI_EVENT_CLASS_PREPARE,
        ]);
    }
    transformer::clear_hooks();
    trampoline::shutdown();
    if let Ok(mut current) = profile_state().write() {
        *current = None;
    }
    status::update(|current| {
        current.installed = false;
        current.events_enabled = false;
        current.trampoline_defined = false;
    });
}

pub fn register_hook(spec: HookSpec) -> Result<(), String> {
    if !ACTIVE.load(Ordering::Acquire) {
        install()?;
    }
    install_trampoline()?;
    transformer::register_hook(spec);
    Ok(())
}





#[cfg(test)]
pub fn hook_registry_test_lock() -> &'static std::sync::Mutex<()> {
    static GATE: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    GATE.get_or_init(|| std::sync::Mutex::new(()))
}



#[cfg(test)]
pub fn register_hook_spec_for_test(spec: HookSpec) {
    transformer::register_hook(spec);
}

pub fn arm_registered_hooks() -> Result<usize, String> {
    if !ACTIVE.load(Ordering::Acquire) {
        install()?;
    }
    install_trampoline()?;
    enable_class_file_hook()?;
    retransform_registered_classes()
}

fn jvmti_error_code(error: raw_jvmti::jvmtiError) -> u32 {
    error as u32
}

fn jvmti_error_label(error: raw_jvmti::jvmtiError) -> &'static str {
    
    match jvmti_error_code(error) {
        0 => "NONE",
        21 => "INVALID_CLASS",
        60 => "INVALID_CLASS_FORMAT",
        61 => "CIRCULAR_CLASS_DEFINITION",
        62 => "FAILS_VERIFICATION",
        63 => "UNSUPPORTED_REDEFINITION_CLASS_ATTRIBUTE_CHANGED",
        64 => "UNSUPPORTED_REDEFINITION_METHOD_DELETED",
        65 => "UNSUPPORTED_REDEFINITION_METHOD_ADDED",
        66 => "UNSUPPORTED_REDEFINITION_METHOD_ATTRIBUTE_CHANGED",
        67 => "UNSUPPORTED_REDEFINITION_CLASS_MODIFIERS_CHANGED",
        68 => "UNSUPPORTED_REDEFINITION_METHOD_MODIFIERS_CHANGED",
        79 => "UNMODIFIABLE_CLASS",
        98 => "NOT_AVAILABLE",
        99 => "MUST_POSSESS_CAPABILITY",
        _ => "UNKNOWN",
    }
}

pub fn retransform_registered_classes() -> Result<usize, String> {
    let env = ENV.load(Ordering::Acquire);
    if env.is_null() || !ACTIVE.load(Ordering::Acquire) {
        return Err("JVMTI is not installed".to_string());
    }
    enable_class_file_hook()?;

    let names = transformer::hooked_class_names();
    if names.is_empty() {
        return Ok(0);
    }

    
    
    
    let outcome = jni_bridge::with_attached(|jni_env| {
        jni_bridge::clear_pending_exception(jni_env);
        let jvmti = unsafe { Jvmti::from_raw(env) };

        crate::diag::log(format!("retransform: attempting {names:?}"));
        let mut ok = 0usize;
        let mut errors = Vec::new();
        
        let mut pins: Vec<jni::objects::GlobalRef> = Vec::new();

        for internal in &names {
            let binary = internal.replace('/', ".");
            let class = match jni_bridge::load_game_class(jni_env, &binary) {
                Ok(class) => class,
                Err(error) => {
                    jni_bridge::clear_pending_exception(jni_env);
                    errors.push(format!("{binary}: load ({error})"));
                    continue;
                }
            };
            jni_bridge::clear_pending_exception(jni_env);

            let global = match jni_env.new_global_ref(&class) {
                Ok(global) => global,
                Err(error) => {
                    jni_bridge::clear_pending_exception(jni_env);
                    errors.push(format!("{binary}: NewGlobalRef ({error})"));
                    continue;
                }
            };
            let raw = global.as_raw() as raw_jni::jclass;

            let status = jvmti.get_class_status(raw).unwrap_or(-1);
            
            if status & 8 != 0 {
                errors.push(format!("{binary}: CLASS_STATUS_ERROR (status={status})"));
                pins.push(global);
                continue;
            }

            match jvmti.is_modifiable_class(raw) {
                Ok(true) => {}
                Ok(false) => {
                    errors.push(format!("{binary}: not modifiable (status={status})"));
                    pins.push(global);
                    continue;
                }
                Err(error) => {
                    errors.push(format!(
                        "{binary}: IsModifiableClass ({} / {}, status={status})",
                        jvmti_error_label(error),
                        jvmti_error_code(error),
                    ));
                    pins.push(global);
                    continue;
                }
            }

            match jvmti.retransform_classes(std::slice::from_ref(&raw)) {
                Ok(()) => ok += 1,
                Err(error) => errors.push(format!(
                    "{binary}: RetransformClasses ({} / {}, status={status})",
                    jvmti_error_label(error),
                    jvmti_error_code(error),
                )),
            }
            pins.push(global);
        }

        let _ = pins;
        Ok((ok, errors))
    });

    match outcome {
        Some((0, errors)) => Err(if errors.is_empty() {
            "no hooked classes could be retransformed".into()
        } else {
            errors.join("; ")
        }),
        Some((ok, errors)) => {
            if !errors.is_empty() {
                status::set_error(format!(
                    "partial retransform ({ok} ok): {}",
                    errors.join("; ")
                ));
            } else {
                status::clear_error();
            }
            Ok(ok)
        }
        None => Err("JNI attach failed while retransforming".into()),
    }
}

pub fn snapshot() -> JvmtiStatus {
    status::snapshot()
}

pub fn last_error() -> String {
    status::snapshot().last_error
}

pub fn profile() -> Option<RuntimeProfile> {
    profile_state()
        .read()
        .ok()
        .and_then(|profile| profile.clone())
}

fn class_name_from_signature(signature: &str) -> Option<&str> {
    signature
        .strip_prefix('L')
        .and_then(|value| value.strip_suffix(';'))
}

#[cfg(test)]
mod tests {
    use super::class_name_from_signature;

    #[test]
    fn extracts_internal_class_name() {
        assert_eq!(
            class_name_from_signature("Lnet/minecraft/client/Minecraft;"),
            Some("net/minecraft/client/Minecraft")
        );
        assert_eq!(class_name_from_signature("[Ljava/lang/String;"), None);
    }
}
