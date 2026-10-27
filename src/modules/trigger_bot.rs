use jni::{JNIEnv, errors::Result as JniResult};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        friends,
        minecraft::{self, MinecraftContext, TargetKind, TargetSearchConfig},
        random, settings, time, tps,
    },
};

const TARGETS_SETTING: &str = "Targets";
const ATTACK_MODE_SETTING: &str = "Attack mode";
const PVP_MODE_SETTING: &str = "PvP mode";
const SPRINT_CONTROL_SETTING: &str = "Sprint control";
const TPS_SYNC_SETTING: &str = "TPS sync";
const CUSTOM_DELAY_SETTING: &str = "Custom delay";
const EXTRA_DELAY_SETTING: &str = "Extra delay";
const ONLY_WITH_WEAPON_SETTING: &str = "Only with weapon";
const HITBOX_SYNC_SETTING: &str = "Hitbox sync";
const ATTACK_DISTANCE_SETTING: &str = "Attack distance";
const ATTACK_INVISIBLE_SETTING: &str = "Attack invisible";
const INVISIBLE_WITH_ARMOR_SETTING: &str = "Invisible with armor";
const ONLY_ARMORED_SETTING: &str = "Only armored";
const LINE_OF_SIGHT_SETTING: &str = "Line of sight";
const BLOCK_CHECK_SETTING: &str = "Block check";
const MISSES_SETTING: &str = "Misses";
const MISS_CHANCE_SETTING: &str = "Miss chance";

const TARGET_ALL: &str = "All";
const TARGET_PLAYERS: &str = "Players";
const TARGET_MOBS: &str = "Mobs";
const ATTACK_CRITS: &str = "Crits";
const ATTACK_SMART: &str = "Smart";
const PVP_18: &str = "1.8";
const PVP_19: &str = "1.9+";
const MISS_TARGET_TIMEOUT_MS: u64 = 2_000;

pub struct TriggerBotModule {
    menu: Module,
    pending_attack_at: Option<u64>,
    pending_target_id: Option<i32>,
    last_target_id: Option<i32>,
    last_attack_at: u64,
    miss_armed: bool,
    miss_at: Option<u64>,
}

impl TriggerBotModule {
    pub fn new() -> Self {
        Self {
            menu: Module::new(
                "TriggerBot",
                "Automatically attacks the entity under your crosshair",
                Category::Combat,
            )
            .with_setting(Setting::choice(
                TARGETS_SETTING,
                [TARGET_ALL, TARGET_PLAYERS, TARGET_MOBS],
                0,
            ))
            .with_setting(Setting::choice(
                ATTACK_MODE_SETTING,
                [ATTACK_CRITS, ATTACK_SMART],
                1,
            ))
            .with_setting(Setting::choice(PVP_MODE_SETTING, [PVP_18, PVP_19], 1))
            .with_setting(Setting::checkbox(SPRINT_CONTROL_SETTING, true))
            .with_setting(Setting::checkbox(TPS_SYNC_SETTING, true))
            .with_setting(Setting::checkbox(CUSTOM_DELAY_SETTING, false))
            .with_setting(Setting::slider(EXTRA_DELAY_SETTING, 0.0, 0.0, 1000.0))
            .with_setting(Setting::checkbox(ONLY_WITH_WEAPON_SETTING, true))
            .with_setting(Setting::checkbox(HITBOX_SYNC_SETTING, false))
            .with_setting(Setting::slider(ATTACK_DISTANCE_SETTING, 3.0, 2.0, 10.0))
            .with_setting(Setting::checkbox(ATTACK_INVISIBLE_SETTING, false))
            .with_setting(Setting::checkbox(INVISIBLE_WITH_ARMOR_SETTING, true))
            .with_setting(Setting::checkbox(ONLY_ARMORED_SETTING, false))
            .with_setting(Setting::checkbox(LINE_OF_SIGHT_SETTING, true))
            .with_setting(Setting::checkbox(BLOCK_CHECK_SETTING, true))
            .with_setting(Setting::checkbox(MISSES_SETTING, false))
            .with_setting(Setting::slider(MISS_CHANCE_SETTING, 5.0, 1.0, 100.0)),
            pending_attack_at: None,
            pending_target_id: None,
            last_target_id: None,
            last_attack_at: 0,
            miss_armed: false,
            miss_at: None,
        }
    }

    fn tick_env(&mut self, env: &mut JNIEnv<'_>, config: &TriggerBotConfig) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            self.clear_pending();
            return Ok(());
        };
        let now = time::now_ms();
        if config.tps_sync {
            tps::sample_game_time(minecraft::level_game_time(&mut ctx)?);
        }

        if !self.can_run(&mut ctx, config)? {
            self.clear_pending();
            return Ok(());
        }

        if self.tick_pending_attack(&mut ctx, config, now)? {
            return Ok(());
        }

        let search_config = TargetSearchConfig {
            target_kind: config.target_kind,
            attack_distance: config.effective_attack_distance() as f64,
            attack_invisible: config.attack_invisible,
            invisible_with_armor: config.invisible_with_armor,
            only_armored: config.only_armored,
            line_of_sight: config.line_of_sight,
        };

        let Some(target_id) = minecraft::find_target_under_crosshair(&mut ctx, &search_config)?
        else {
            self.handle_miss(&mut ctx, config, now)?;
            return Ok(());
        };
        if minecraft::vanilla_crosshair_entity_id(&mut ctx)? != Some(target_id) {
            self.handle_miss(&mut ctx, config, now)?;
            return Ok(());
        }

        let Some(target) = ctx.entity_by_id(target_id)? else {
            return Ok(());
        };

        if self.can_attack(&mut ctx, &target, config)? {
            let delay = self.attack_delay(config);
            if delay == 0 {
                self.perform_attack(&mut ctx, &target, target_id, config, now)?;
            } else {
                self.pending_target_id = Some(target_id);
                self.pending_attack_at = Some(now + delay);
            }
        } else {
        }

        Ok(())
    }

    fn can_run(
        &self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &TriggerBotConfig,
    ) -> JniResult<bool> {
        Ok(minecraft::has_player_and_world(ctx)
            && !minecraft::player_is_using_item(ctx)?
            && (!config.only_with_weapon || minecraft::player_has_weapon(ctx)?))
    }

    fn tick_pending_attack(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &TriggerBotConfig,
        now: u64,
    ) -> JniResult<bool> {
        let (Some(target_id), Some(attack_at)) = (self.pending_target_id, self.pending_attack_at)
        else {
            return Ok(false);
        };

        if now < attack_at {
            return Ok(true);
        }

        let Some(target) = ctx.entity_by_id(target_id)? else {
            self.clear_pending();
            return Ok(true);
        };

        let invalid = !minecraft::entity_is_alive(ctx, &target)?
            || !self.can_run(ctx, config)?
            || friends::no_friend_damage_active() && minecraft::entity_is_friend(ctx, &target)?
            || minecraft::target_distance(ctx, &target)?
                > config.effective_attack_distance() as f64;

        if invalid {
            self.clear_pending();
            return Ok(true);
        }

        if minecraft::vanilla_crosshair_entity_id(ctx)? != Some(target_id) {
            self.clear_pending();
            return Ok(true);
        }

        if !self.can_attack(ctx, &target, config)? {
            return Ok(true);
        }

        let attack_result = self.perform_attack(ctx, &target, target_id, config, now);
        self.clear_pending();
        attack_result?;
        Ok(true)
    }

    fn can_attack(
        &self,
        ctx: &mut MinecraftContext<'_, '_>,
        _target: &jni::objects::JObject<'_>,
        config: &TriggerBotConfig,
    ) -> JniResult<bool> {
        if config.block_check && minecraft::player_in_blocking_block(ctx)? {
            return Ok(false);
        }

        let player_blocking = ctx
            .env
            .call_method(&ctx.player, "isBlocking", "()Z", &[])
            .map(|v| v.z().unwrap_or(false))
            .unwrap_or(false);
        if player_blocking {
            return Ok(false);
        }

        let special = minecraft::player_in_special_movement(ctx)?;
        let critical = minecraft::player_can_critical(ctx)?;

        if !config.legacy_pvp {
            let cooldown_gate = if config.tps_sync {
                (0.8 * tps::tick_multiplier()).min(1.0)
            } else {
                0.8
            };
            let on_ground = minecraft::player_on_ground(ctx)?;
            let cooldown_threshold = if on_ground { 0.92 } else { 0.95 };
            let min_interval = if on_ground { 120u64 } else { 400u64 };
            if minecraft::attack_strength(ctx)? < cooldown_threshold.max(cooldown_gate) {
                return Ok(false);
            }
            if time::now_ms().saturating_sub(self.last_attack_at) < min_interval {
                return Ok(false);
            }
        }

        if config.crits_only {
            return Ok(special || critical);
        }

        if config.legacy_pvp {
            return Ok(true);
        }

        if !special && minecraft::player_velocity_y(ctx)? > 0.0 {
            return Ok(false);
        }

        Ok(minecraft::player_on_ground(ctx)? || special || critical)
    }

    fn perform_attack(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        target: &jni::objects::JObject<'_>,
        target_id: i32,
        config: &TriggerBotConfig,
        now: u64,
    ) -> JniResult<()> {
        let _ = target;
        if config.sprint_control {
            let _ = minecraft::set_player_sprinting(ctx, false);
        }
        minecraft::queue_vanilla_attack_click(ctx)?;

        self.last_target_id = Some(target_id);
        self.last_attack_at = now;
        self.miss_armed = true;
        self.miss_at = None;
        Ok(())
    }

    fn handle_miss(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &TriggerBotConfig,
        now: u64,
    ) -> JniResult<()> {
        if !config.misses {
            return Ok(());
        }

        let Some(target_id) = self.last_target_id else {
            return Ok(());
        };
        let Some(target) = ctx.entity_by_id(target_id)? else {
            self.last_target_id = None;
            return Ok(());
        };

        if !minecraft::entity_is_alive(ctx, &target)?
            || now.saturating_sub(self.last_attack_at) > MISS_TARGET_TIMEOUT_MS
            || minecraft::player_distance_to_entity(ctx, &target)?
                > config.effective_attack_distance() + 1.0
        {
            self.last_target_id = None;
            self.miss_at = None;
            self.miss_armed = false;
            return Ok(());
        }

        if self.miss_armed {
            self.miss_armed = false;
            if random::chance_percent(config.miss_chance) {
                self.miss_at = Some(now);
            }
        }

        if self.miss_at.is_some_and(|miss_at| now >= miss_at)
            && minecraft::attack_strength(ctx)? >= 0.8
        {
            minecraft::swing_main_hand(ctx)?;
            minecraft::reset_attack_strength(ctx)?;
            self.miss_at = None;
        }

        Ok(())
    }

    fn attack_delay(&self, config: &TriggerBotConfig) -> u64 {
        let mut delay = if config.custom_delay {
            config.extra_delay.max(0.0) as u64
        } else {
            0
        };

        if config.tps_sync {
            let multiplier = tps::tick_multiplier();
            if multiplier > 1.0 {
                delay = delay.max(((multiplier - 1.0) * 500.0) as u64);
            }
            if delay > 0 {
                delay = tps::adjust_delay(delay);
            }
        }

        delay
    }

    fn clear_pending(&mut self) {
        self.pending_attack_at = None;
        self.pending_target_id = None;
    }

    fn reset_state(&mut self) {
        self.clear_pending();
        self.last_target_id = None;
        self.last_attack_at = 0;
        self.miss_armed = false;
        self.miss_at = None;
    }
}

impl ClientModule for TriggerBotModule {
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

    fn on_tick(&mut self) {
        let config = TriggerBotConfig::from_module(&self.menu);
        let _ = jni_bridge::with_env(|env| {
            let result = self.tick_env(env, &config);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}

#[derive(Clone)]
struct TriggerBotConfig {
    target_kind: TargetKind,
    crits_only: bool,
    legacy_pvp: bool,
    sprint_control: bool,
    tps_sync: bool,
    custom_delay: bool,
    extra_delay: f32,
    only_with_weapon: bool,
    hitbox_sync: bool,
    attack_distance: f32,
    attack_invisible: bool,
    invisible_with_armor: bool,
    only_armored: bool,
    line_of_sight: bool,
    block_check: bool,
    misses: bool,
    miss_chance: f32,
}

impl TriggerBotConfig {
    fn effective_attack_distance(&self) -> f32 {
        let _hitbox_sync_requested = self.hitbox_sync;
        self.attack_distance
    }

    fn from_module(module: &Module) -> Self {
        let targets = settings::choice(module, TARGETS_SETTING, TARGET_ALL);
        let attack_mode = settings::choice(module, ATTACK_MODE_SETTING, ATTACK_SMART);
        let pvp_mode = settings::choice(module, PVP_MODE_SETTING, PVP_19);

        Self {
            target_kind: match targets.as_str() {
                TARGET_PLAYERS => TargetKind::Players,
                TARGET_MOBS => TargetKind::Mobs,
                _ => TargetKind::All,
            },
            crits_only: attack_mode == ATTACK_CRITS,
            legacy_pvp: pvp_mode == PVP_18,
            sprint_control: settings::checkbox(module, SPRINT_CONTROL_SETTING, true),
            tps_sync: settings::checkbox(module, TPS_SYNC_SETTING, true),
            custom_delay: settings::checkbox(module, CUSTOM_DELAY_SETTING, false),
            extra_delay: settings::slider(module, EXTRA_DELAY_SETTING, 0.0),
            only_with_weapon: settings::checkbox(module, ONLY_WITH_WEAPON_SETTING, true),
            hitbox_sync: settings::checkbox(module, HITBOX_SYNC_SETTING, false),
            attack_distance: settings::slider(module, ATTACK_DISTANCE_SETTING, 3.0),
            attack_invisible: settings::checkbox(module, ATTACK_INVISIBLE_SETTING, false),
            invisible_with_armor: settings::checkbox(module, INVISIBLE_WITH_ARMOR_SETTING, true),
            only_armored: settings::checkbox(module, ONLY_ARMORED_SETTING, false),
            line_of_sight: settings::checkbox(module, LINE_OF_SIGHT_SETTING, true),
            block_check: settings::checkbox(module, BLOCK_CHECK_SETTING, true),
            misses: settings::checkbox(module, MISSES_SETTING, false),
            miss_chance: settings::slider(module, MISS_CHANCE_SETTING, 5.0),
        }
    }
}
