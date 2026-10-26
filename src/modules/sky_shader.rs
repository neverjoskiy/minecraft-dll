use std::sync::{Mutex, OnceLock};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{minecraft::MinecraftContext, name_tags, settings},
};

pub const MODE_SETTING: &str = "Mode";
pub const MODE_MARBLING: &str = "Marbling";
pub const MODE_WARP_FBM: &str = "Warp FBM";
pub const MODE_AURORAS: &str = "Auroras";
pub const SPEED_SETTING: &str = "Speed";
pub const INTENSITY_SETTING: &str = "Intensity";
pub const SCALE_SETTING: &str = "Scale";
pub const THEME_SYNC_SETTING: &str = "Theme sync";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkyShaderMode {
    Marbling,
    WarpFbm,
    Auroras,
}

impl SkyShaderMode {
    pub fn from_name(name: &str) -> Self {
        match name {
            MODE_WARP_FBM => Self::WarpFbm,
            MODE_AURORAS => Self::Auroras,
            _ => Self::Marbling,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Marbling => 0,
            Self::WarpFbm => 1,
            Self::Auroras => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkyCamera {
    pub forward: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub fov_y_degrees: f32,
}

impl Default for SkyCamera {
    fn default() -> Self {
        Self {
            forward: [0.0, 0.0, 1.0],
            yaw_degrees: 0.0,
            pitch_degrees: 0.0,
            fov_y_degrees: 70.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkyShaderConfig {
    pub mode: SkyShaderMode,
    pub speed: f32,
    pub intensity: f32,
    pub scale: f32,
    pub sync_theme: bool,
    pub shader_color: [f32; 3],
    pub background_color: [f32; 3],
    pub camera: SkyCamera,
}

impl SkyShaderConfig {
    pub fn from_module(module: &Module, camera: SkyCamera) -> Self {
        let sync_theme = settings::checkbox(module, THEME_SYNC_SETTING, true);
        let (shader_color, background_color) = if sync_theme {
            let theme = crate::theme_colors::current();
            (theme.accent, theme.extreme)
        } else {
            ([0.55, 0.65, 0.95], [0.18, 0.22, 0.35])
        };
        Self {
            mode: SkyShaderMode::from_name(&settings::choice(module, MODE_SETTING, MODE_MARBLING)),
            speed: settings::slider(module, SPEED_SETTING, 1.0),
            intensity: settings::slider(module, INTENSITY_SETTING, 1.0),
            scale: settings::slider(module, SCALE_SETTING, 1.0),
            sync_theme,
            shader_color,
            background_color,
            camera,
        }
    }
}

fn camera_cache() -> &'static Mutex<SkyCamera> {
    static CACHE: OnceLock<Mutex<SkyCamera>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SkyCamera::default()))
}

fn store_camera(camera: SkyCamera) {
    if let Ok(mut slot) = camera_cache().lock() {
        *slot = camera;
    }
}

fn cached_camera() -> SkyCamera {
    camera_cache().lock().map(|slot| *slot).unwrap_or_default()
}


pub fn refresh_camera_cache() -> SkyCamera {
    let _ = jni_bridge::with_env(|env| {
        let result = (|| {
            let Some(mut ctx) = MinecraftContext::new(env)? else {
                return Ok(());
            };
            let view = name_tags::camera_view(&mut ctx)?;
            store_camera(SkyCamera {
                forward: [
                    view.forward.x as f32,
                    view.forward.y as f32,
                    view.forward.z as f32,
                ],
                yaw_degrees: view.yaw_degrees as f32,
                pitch_degrees: view.pitch_degrees as f32,
                fov_y_degrees: view.fov_y_degrees.clamp(1.0, 170.0) as f32,
            });
            Ok(())
        })();
        if result.is_err() {
            jni_bridge::clear_pending_exception(env);
        }
        result
    });
    cached_camera()
}

pub struct SkyShaderModule {
    menu: Module,
}

impl SkyShaderModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "SkyShader",
                "Shadertoy sky — replaces vanilla sky disc before terrain",
                Category::Render,
            )
            .with_setting(Setting::choice(
                MODE_SETTING,
                [MODE_MARBLING, MODE_WARP_FBM, MODE_AURORAS],
                0,
            ))
            .with_setting(Setting::slider(SPEED_SETTING, 1.0, 0.05, 5.0))
            .with_setting(Setting::slider(INTENSITY_SETTING, 1.0, 0.1, 3.0))
            .with_setting(Setting::slider(SCALE_SETTING, 1.0, 0.25, 3.0))
            .with_setting(Setting::checkbox(THEME_SYNC_SETTING, true)),
        }
    }

    fn refresh_camera(&self) {
        refresh_camera_cache();
    }
}

impl ClientModule for SkyShaderModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }
}

fn config_cache() -> &'static Mutex<Option<SkyShaderConfig>> {
    static CACHE: OnceLock<Mutex<Option<SkyShaderConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn refresh_draw_snapshot() -> bool {
    let camera = refresh_camera_cache();
    let config = crate::module_system::with_registry(|registry| {
        registry
            .modules()
            .iter()
            .map(|module| module.menu_module())
            .find(|module| module.name == "SkyShader" && module.enabled)
            .map(|module| SkyShaderConfig::from_module(module, camera))
    })
    .flatten();
    let active = config.is_some();
    if let Ok(mut cached) = config_cache().lock() {
        *cached = config;
    }
    active
}

pub fn draw_snapshot() -> Option<SkyShaderConfig> {
    config_cache().lock().ok().and_then(|cached| *cached)
}
