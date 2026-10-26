



















pub mod geometry;
mod hooks;

pub use hooks::{on_submit_world, register_world_hooks, world_hook_spec};

use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;


static ARMED: AtomicBool = AtomicBool::new(false);

static UNAVAILABLE: AtomicBool = AtomicBool::new(false);


pub fn enabled() -> bool {
    ARMED.load(Ordering::Acquire) && !UNAVAILABLE.load(Ordering::Acquire)
}

pub fn armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

pub fn mark_unavailable(reason: impl Into<String>) {
    let reason = reason.into();
    UNAVAILABLE.store(true, Ordering::Release);
    crate::diag::log(format!("world: UNAVAILABLE {reason}"));
    crate::native_jvmti::status::set_error(format!("world render: {reason}"));
}





pub fn ensure_armed() -> Result<(), String> {
    if ARMED.load(Ordering::Acquire) {
        return Ok(());
    }
    if UNAVAILABLE.load(Ordering::Acquire) {
        return Err("world render unavailable".into());
    }
    
    
    if !crate::jni_bridge::is_world_active() {
        crate::diag::log_once("world_wait_world", "world: waiting for world before arming");
        return Err("join a world first".into());
    }
    crate::diag::log("world: arming (jvmti install)");
    crate::native_jvmti::install().map_err(|error| {
        crate::diag::log(format!("world: jvmti install failed: {error}"));
        format!("jvmti: {error}")
    })?;
    crate::native_jvmti::install_trampoline().map_err(|error| {
        crate::diag::log(format!("world: trampoline failed: {error}"));
        format!("trampoline: {error}")
    })?;

    
    
    crate::avatar::render::register_avatar_hooks_if_active()?;
    register_world_hooks()?;
    crate::diag::log(format!(
        "world: hooks registered, classes={:?}",
        crate::native_jvmti::hooked_class_names()
    ));

    crate::native_jvmti::status::clear_error();
    if let Err(error) = crate::native_jvmti::arm_registered_hooks() {
        mark_unavailable(format!("arm failed: {error}"));
        return Err(format!("arm failed: {error}"));
    }
    
    
    
    let error = crate::native_jvmti::status::snapshot().last_error;
    if error.contains("FAILS_VERIFICATION") {
        mark_unavailable(format!("hook class rejected: {error}"));
        return Err(format!("hook class rejected: {error}"));
    }
    ARMED.store(true, Ordering::Release);
    crate::diag::log(format!(
        "world: ARMED transformed={} status={error:?}",
        crate::native_jvmti::status::snapshot().transformed_classes
    ));
    crate::native_jvmti::status::update(|status| {
        status.last_error = "world render armed".to_string();
    });
    Ok(())
}




pub fn skip_player(player: Uuid) -> bool {
    
    
    
    
    
    
    if !crate::avatar::render::replacement_enabled() {
        return false;
    }
    crate::avatar::runtime::active_avatar_id(player).is_some()
}



static LOCAL_PLAYER: std::sync::Mutex<Option<Uuid>> = std::sync::Mutex::new(None);

pub(crate) fn set_local_player(player: Option<Uuid>) {
    if let Ok(mut slot) = LOCAL_PLAYER.lock() {
        *slot = player;
    }
}







#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_not_submitted_before_hooks_exist() {
        
        if !armed() {
            assert!(!enabled());
        }
    }

    fn world_hook_present() -> bool {
        crate::native_jvmti::hooked_class_names()
            .iter()
            .any(|name| name == crate::native_jvmti::RENDER_SYMBOLS.level_renderer)
    }

    
    
    
    
    
    #[test]
    fn world_hook_survives_an_avatar_hook_rebuild() {
        
        
        
        let _lock = crate::native_jvmti::hook_registry_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        
        
        crate::native_jvmti::clear_hooks();
        crate::native_jvmti::register_hook_spec_for_test(world_hook_spec());
        assert!(
            world_hook_present(),
            "world hook must be registered"
        );

        
        ARMED.store(true, Ordering::Release);
        crate::native_jvmti::clear_hooks();
        crate::avatar::render::readd_world_hooks_for_test();

        assert!(
            world_hook_present(),
            "avatar rebuild dropped the world hook"
        );
        ARMED.store(false, Ordering::Release);
        crate::native_jvmti::clear_hooks();
    }

    
    
    
    
    
    
    
    
    
    #[test]
    fn world_render_jni_symbols_exist_in_the_client() {
        use std::io::Read;

        let Ok(path) = std::env::var("XZ_MC_26_2_CLIENT_JAR") else {
            return;
        };
        let file = std::fs::File::open(path).expect("open client fixture");
        let mut archive = zip::ZipArchive::new(file).expect("open client jar");

        let read_class = |archive: &mut zip::ZipArchive<std::fs::File>, name: &str| -> String {
            let mut entry = archive
                .by_name(&format!("{name}.class"))
                .unwrap_or_else(|_| panic!("{name} missing from client jar"));
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read class");
            
            
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let camera = read_class(&mut archive, "net/minecraft/client/Camera");
        assert!(
            camera.contains("position"),
            "Camera.position field is gone; camera_position would silently return None"
        );

        let game_renderer = read_class(&mut archive, "net/minecraft/client/renderer/GameRenderer");
        assert!(
            game_renderer.contains("mainCamera"),
            "GameRenderer.mainCamera() is gone — do NOT switch to getMainCamera(), verify first"
        );

        let render_types =
            read_class(&mut archive, "net/minecraft/client/renderer/rendertype/RenderTypes");
        for factory in ["lines", "debugFilledBox"] {
            assert!(
                render_types.contains(factory),
                "RenderTypes.{factory}() missing; world batches would find no render type"
            );
        }

        let collector = read_class(
            &mut archive,
            "net/minecraft/client/renderer/OrderedSubmitNodeCollector",
        );
        assert!(
            collector.contains("submitCustomGeometry"),
            "submitCustomGeometry is gone; the whole world-render path is dead"
        );
    }

    #[test]
    fn unavailable_blocks_enabled_even_when_armed() {
        let armed_before = ARMED.load(Ordering::Acquire);
        let unavailable_before = UNAVAILABLE.load(Ordering::Acquire);
        ARMED.store(true, Ordering::Release);
        UNAVAILABLE.store(true, Ordering::Release);
        assert!(!enabled());
        ARMED.store(armed_before, Ordering::Release);
        UNAVAILABLE.store(unavailable_before, Ordering::Release);
    }
}
