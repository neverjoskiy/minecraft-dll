use jni::{JNIEnv, errors::Result as JniResult};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        minecraft::{self, MinecraftContext},
        settings,
    },
};

const DELAY_SETTING: &str = "Delay";
const THROW_BIND_SETTING: &str = "Throw key";

pub struct KeyPearlModule {
    menu: Module,
    stage: PearlStage,
    delay_ticks: i32,
    pearl_slot: i32,
    previous_slot: i32,
    pearl_in_hotbar: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PearlStage {
    Idle,
    SelectHotbar,
    UseHotbar,
    RestoreHotbar,
    SwapIn,
    UseSwapped,
    SwapBack,
}

impl KeyPearlModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "KeyPearl",
                "Throws an ender pearl by key, swapping it from inventory when needed",
                Category::Player,
            )
            .with_setting(Setting::action_bind(THROW_BIND_SETTING, "None", 0))
            .with_setting(Setting::slider(DELAY_SETTING, 1.0, 0.0, 5.0)),
            stage: PearlStage::Idle,
            delay_ticks: 0,
            pearl_slot: -1,
            previous_slot: -1,
            pearl_in_hotbar: false,
        }
    }

    fn begin_env(&mut self, env: &mut JNIEnv<'_>) -> JniResult<bool> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(false);
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(false);
        }

        let Some(pearl_slot) = minecraft::find_ender_pearl_inventory_menu_slot(&mut ctx)? else {
            return Ok(false);
        };
        let previous_slot = minecraft::selected_hotbar_slot(&mut ctx)?;
        self.pearl_slot = pearl_slot;
        self.previous_slot = previous_slot;
        self.pearl_in_hotbar = (36..=44).contains(&pearl_slot);
        self.stage = if self.pearl_in_hotbar {
            PearlStage::SelectHotbar
        } else {
            PearlStage::SwapIn
        };
        self.delay_ticks = 0;
        Ok(true)
    }

    fn tick_env(&mut self, env: &mut JNIEnv<'_>) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            self.finish();
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            self.finish();
            return Ok(());
        }

        if self.delay_ticks > 0 {
            self.delay_ticks -= 1;
            return Ok(());
        }

        match self.stage {
            PearlStage::Idle => self.finish(),
            PearlStage::SelectHotbar => {
                minecraft::set_selected_hotbar_slot(&mut ctx, self.pearl_slot - 36)?;
                self.advance(PearlStage::UseHotbar);
            }
            PearlStage::UseHotbar => {
                minecraft::use_main_hand_item(&mut ctx)?;
                self.advance(PearlStage::RestoreHotbar);
            }
            PearlStage::RestoreHotbar => {
                minecraft::set_selected_hotbar_slot(&mut ctx, self.previous_slot)?;
                self.finish();
            }
            PearlStage::SwapIn => {
                minecraft::swap_inventory_menu_slot_to_hotbar(
                    &mut ctx,
                    self.pearl_slot,
                    self.previous_slot,
                )?;
                self.advance(PearlStage::UseSwapped);
            }
            PearlStage::UseSwapped => {
                minecraft::use_main_hand_item(&mut ctx)?;
                self.advance(PearlStage::SwapBack);
            }
            PearlStage::SwapBack => {
                minecraft::swap_inventory_menu_slot_to_hotbar(
                    &mut ctx,
                    self.pearl_slot,
                    self.previous_slot,
                )?;
                self.finish();
            }
        }

        Ok(())
    }

    fn advance(&mut self, next: PearlStage) {
        self.stage = next;
        self.delay_ticks = settings::slider(&self.menu, DELAY_SETTING, 1.0).round() as i32;
    }

    fn finish(&mut self) {
        self.stage = PearlStage::Idle;
        self.delay_ticks = 0;
        self.pearl_slot = -1;
        self.previous_slot = -1;
        self.pearl_in_hotbar = false;
    }

    fn reset_state(&mut self) {
        self.stage = PearlStage::Idle;
        self.delay_ticks = 0;
        self.pearl_slot = -1;
        self.previous_slot = -1;
        self.pearl_in_hotbar = false;
    }
}

impl ClientModule for KeyPearlModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_enable(&mut self) {
        self.reset_state();
    }

    fn on_disable(&mut self) {
        self.reset_state();
    }

    fn on_action_bind(&mut self, name: &str) {
        if name != THROW_BIND_SETTING || self.stage != PearlStage::Idle {
            return;
        }

        let started = jni_bridge::with_env(|env| {
            let result = self.begin_env(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        })
        .unwrap_or(false);

        if !started {
            self.reset_state();
        }
    }

    fn on_tick(&mut self) {
        if self.stage == PearlStage::Idle {
            return;
        }

        let _ = jni_bridge::with_env(|env| {
            let result = self.tick_env(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
                self.finish();
            }
            result
        });
    }
}
