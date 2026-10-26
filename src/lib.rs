#![allow(non_snake_case)]

mod anti_detect;
mod avatar;
mod core;
mod gui;
mod platform;
mod utils;
mod world_render;
mod modules;


pub use core::config;
pub use core::diag;
pub use core::interface;
pub use core::module_system;
pub use core::theme_colors;
pub use gui::anim as gui_anim;
pub use gui::blur as gui_blur;
pub use gui::hud as gui_hud;
pub use gui::icons as gui_icons;
pub use gui::sky_shader as gui_sky_shader;
pub use platform::hooks;
pub use platform::jni_bridge;
pub use platform::jvmti as native_jvmti;

use std::ffi::c_void;

use module_system::{ModuleRegistry, set_registry};
use windows::Win32::{
    Foundation::HINSTANCE,
    System::{
        LibraryLoader::DisableThreadLibraryCalls,
        SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH},
        Threading::CreateThread,
    },
};

#[unsafe(no_mangle)]
extern "system" fn DllMain(dll_module: HINSTANCE, call_code: u32, _reserved: *mut c_void) -> bool {
    match call_code {
        DLL_PROCESS_ATTACH => unsafe {
            let _ = DisableThreadLibraryCalls(dll_module.into());
            diag::set_module(dll_module);
            diag::reset();
            let _ = CreateThread(
                None,
                0,
                Some(init_thread),
                Some(dll_module.0 as *const c_void),
                Default::default(),
                None,
            );
        },
        DLL_PROCESS_DETACH => unsafe {
            let _ = module_system::with_registry(|registry| registry.save_config());
            native_jvmti::shutdown();
            hooks::shutdown();
        },
        _ => {}
    }

    true
}

unsafe extern "system" fn init_thread(_module: *mut c_void) -> u32 {
    let mut registry = ModuleRegistry::new();

    
    registry.register(Box::new(modules::HudModule::new()));
    registry.register(Box::new(modules::NameTagsModule::new()));
    registry.register(Box::new(modules::ChamsModule::new()));
    registry.register(Box::new(modules::NoOverlayModule::new()));
    registry.register(Box::new(modules::FullBrightModule::new()));
    registry.register(Box::new(modules::AutoSprintModule::new()));
    registry.register(Box::new(modules::ChangeTimeModule::new()));
    registry.register(Box::new(modules::SkyShaderModule::new()));
    registry.register(Box::new(modules::TargetEspModule::new()));
    registry.register(Box::new(modules::TrajectoriesModule::new()));
    registry.register(Box::new(modules::ChinaHatModule::new()));
    registry.register(Box::new(modules::KeyPearlModule::new()));
    registry.register(Box::new(modules::AttackAuraModule::new()));
    registry.register(Box::new(modules::AimAssistModule::new()));
    registry.register(Box::new(modules::NoFriendDamageModule::new()));
    registry.register(Box::new(modules::NoJumpDelayModule::new()));
    registry.register(Box::new(modules::TriggerBotModule::new()));
    registry.register(Box::new(modules::JumpCircleModule::new()));
    registry.load_config();

    let _ = set_registry(registry);

    
    
    
    native_jvmti::status::set_error("jvmti idle — Activate avatar in-world to arm");

    let _ = hooks::install();
    0
}
