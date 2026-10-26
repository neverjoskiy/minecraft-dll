
use crate::{
    interface::{Category, Module, Setting},
    module_system::ClientModule,
    utils::{aim_target, settings, world_markers},
};

pub const MODE_SETTING: &str = "Режим";
pub const MODE_IMAGE1: &str = "Картинка 1";
pub const MODE_IMAGE2: &str = "Картинка 2";
pub const MODE_RING: &str = "Кольцо";
pub const MODE_SOULS: &str = "Души";
pub const MODE_CUBES: &str = "Кубы";
pub const MODE_CRYSTALS: &str = "Кристаллы";
pub const MODE_RIDER: &str = "Райдер";
pub const MODE_GHOSTS: &str = "Призраки";

pub const SIZE_SETTING: &str = "Размер";
pub const ROTATE_SPEED_SETTING: &str = "Скорость вращения";
pub const HURT_COLOR_SETTING: &str = "Окрашивание при ударе";
pub const RING_RADIUS_SETTING: &str = "Радиус кольца";
pub const RING_SPEED_SETTING: &str = "Скорость кольца";
pub const GHOST_COUNT_SETTING: &str = "Кол-во призраков";
pub const GHOST_LIFE_SETTING: &str = "Время жизни (мс)";
pub const STRENGTH_XZ_SETTING: &str = "Цикл XZ";
pub const STRENGTH_Y_SETTING: &str = "Цикл Y";

pub struct TargetEspModule {
    menu: Module,
}

impl TargetEspModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new("TargetESP", "Wonderful TargetESP — все режимы: картинки, кольцо, души, кубы, кристаллы, райдер", Category::Render)
                .with_setting(Setting::choice(MODE_SETTING, [MODE_IMAGE1, MODE_IMAGE2, MODE_RING, MODE_SOULS, MODE_CUBES, MODE_CRYSTALS, MODE_RIDER, MODE_GHOSTS], 0))
                .with_setting(Setting::slider(SIZE_SETTING, 1.15, 0.6, 2.5))
                .with_setting(Setting::slider(ROTATE_SPEED_SETTING, 1.2, 0.2, 4.0))
                .with_setting(Setting::checkbox(HURT_COLOR_SETTING, true))
                .with_setting(Setting::slider(RING_RADIUS_SETTING, 0.5, 0.3, 1.5))
                .with_setting(Setting::slider(RING_SPEED_SETTING, 1.0, 0.3, 3.0))
                .with_setting(Setting::slider(GHOST_COUNT_SETTING, 3.0, 2.0, 5.0))
                .with_setting(Setting::slider(GHOST_LIFE_SETTING, 350.0, 150.0, 500.0))
                .with_setting(Setting::slider(STRENGTH_XZ_SETTING, 2000.0, 1000.0, 5000.0))
                .with_setting(Setting::slider(STRENGTH_Y_SETTING, 1700.0, 1000.0, 5000.0))
                .with_setting(Setting::color_picker("Color", [0.65, 0.52, 1.0, 1.0])),
        }
    }
}

impl ClientModule for TargetEspModule {
    fn menu_module(&self) -> &Module { &self.menu }
    fn menu_module_mut(&mut self) -> &mut Module { &mut self.menu }
    fn on_frame(&mut self) {
        let overlay = aim_target::overlay_state();
        if !overlay.target_active {
            world_markers::clear_target_esp_config();
            return;
        }
        let mode = settings::choice(&self.menu, MODE_SETTING, MODE_IMAGE1);
        let size = settings::slider(&self.menu, SIZE_SETTING, 1.15);
        let speed = settings::slider(&self.menu, ROTATE_SPEED_SETTING, 1.2);
        let hurt = settings::checkbox(&self.menu, HURT_COLOR_SETTING, true);
        let ring_radius = settings::slider(&self.menu, RING_RADIUS_SETTING, 0.5);
        let ring_speed = settings::slider(&self.menu, RING_SPEED_SETTING, 1.0);
        let ghost_count = settings::slider(&self.menu, GHOST_COUNT_SETTING, 3.0) as i32;
        let ghost_life = settings::slider(&self.menu, GHOST_LIFE_SETTING, 350.0) as u64;
        let strength_xz = settings::slider(&self.menu, STRENGTH_XZ_SETTING, 2000.0);
        let strength_y = settings::slider(&self.menu, STRENGTH_Y_SETTING, 1700.0);
        let color = settings::color_picker(&self.menu, "Color", [0.65, 0.52, 1.0, 1.0]);
        world_markers::set_target_esp_config(Some(world_markers::TargetEspConfig {
            mode: mode.clone(),
            size,
            rotate_speed: speed,
            hurt_color: hurt,
            ring_radius,
            ring_speed,
            ghost_count,
            ghost_life_ms: ghost_life,
            strength_xz,
            strength_y,
            color,
        }));
    }
    fn on_disable(&mut self) { world_markers::clear_target_esp_config(); }
}
