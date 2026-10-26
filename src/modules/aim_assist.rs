use std::{f64::consts::TAU, time::Instant};

use jni::{JNIEnv, errors::Result as JniResult};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        aim_target::{self, AimAssistOverlay, AimPriority, AimTarget, AimTargetConfig},
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        neuro::{
            AimAuthority, NeuroController, NeuroFrameInput, OutputInterpolator, RotationMode,
            blend_with_classic, clamp_delta_toward_error,
        },
        random, rotation,
        rotation::Angle,
        settings, time,
    },
};

const DISTANCE_SETTING: &str = "Distance";
const FOV_SETTING: &str = "FOV";
const DRAW_FOV_SETTING: &str = "Draw FOV radius";
const SPEED_SETTING: &str = "Speed";
const PRIORITY_SETTING: &str = "Priority";
const ONLY_WITH_WEAPON_SETTING: &str = "Only with weapon";
const ONLY_PLAYERS_SETTING: &str = "Only players";
const NO_AIM_IN_INVENTORY_SETTING: &str = "No aim in inventory";
const HIT_INVISIBLE_SETTING: &str = "Hit invisible";
const ONLY_ARMORED_SETTING: &str = "Only armored";
const IGNORE_NAKED_SETTING: &str = "Ignore naked";
const ONLY_X_SETTING: &str = "Only X";
const MULTIPOINT_SETTING: &str = "Multipoint";
const WALL_CHECK_SETTING: &str = "Wall check";
const DISABLE_ON_WORLD_CHANGE_SETTING: &str = "Disable on world change";
const MICRO_MOVEMENTS_SETTING: &str = "Micro movements";
const ROTATION_MODE_SETTING: &str = "Rotation Mode";
const MAX_SAMPLES_SETTING: &str = "Neuro max samples";
const RESET_NEURO_SETTING: &str = "Reset Neuro profile";

const PRIORITY_DISTANCE: &str = "Distance";
const PRIORITY_HEALTH: &str = "Health";
const MODE_DEFAULT: &str = "Default";
const MODE_NEURO: &str = "Neuro";

pub struct AimAssistModule {
    menu: Module,
    target_id: Option<i32>,
    target_acquired_at: u64,
    last_frame_at: Option<Instant>,
    previous_yaw_delta: f64,
    previous_pitch_delta: f64,
    yaw_phase: f64,
    pitch_phase: f64,
    yaw_phase_speed: f64,
    pitch_phase_speed: f64,
    ease_power: f64,
    speed_jitter: f64,
    aim_height_kick: f64,
    ease_variant: i32,
    aim_height_noise: f64,
    aim_height_noise_at: u64,
    current_point_x: f64,
    current_point_y: f64,
    current_point_z: f64,
    target_point_x: f64,
    target_point_y: f64,
    target_point_z: f64,
    point_update_at: u64,
    point_lerp: f64,
    dimension_key: Option<String>,
    scatter_x: f64,
    scatter_y: f64,
    scatter_at: u64,
    can_aim_at: u64,
    last_target_switch_at: u64,
    neuro: NeuroController,
    last_rotation: Option<Angle>,
    last_attack_clicks: i32,
    last_human_yaw_delta: f64,
    last_human_pitch_delta: f64,
    prev_prev_human_yaw_delta: f64,
    prev_prev_human_pitch_delta: f64,
    prev_yaw_error: f64,
    prev_pitch_error: f64,
    prev_target_pos: Option<Vec3d>,
    last_attack_at: u64,
    frames_since_acquire: f64,
    last_frame_dt: f64,
    session_break_pending: bool,
    neuro_output: OutputInterpolator,
}

impl AimAssistModule {
    pub fn new() -> Self {
        let now = time::now_ms();
        let mut module = Self {
            menu: Module::new(
                "AimAssist",
                "Smoothly aims at valid targets inside the selected FOV",
                Category::Combat,
            )
            .with_setting(Setting::slider(DISTANCE_SETTING, 4.0, 1.0, 6.0))
            .with_setting(Setting::slider(FOV_SETTING, 90.0, 20.0, 360.0))
            .with_setting(Setting::checkbox(DRAW_FOV_SETTING, false))
            .with_setting(Setting::slider(SPEED_SETTING, 2.0, 0.1, 10.0))
            .with_setting(Setting::choice(
                PRIORITY_SETTING,
                [PRIORITY_DISTANCE, PRIORITY_HEALTH],
                0,
            ))
            .with_setting(Setting::checkbox(ONLY_WITH_WEAPON_SETTING, true))
            .with_setting(Setting::checkbox(ONLY_PLAYERS_SETTING, true))
            .with_setting(Setting::checkbox(NO_AIM_IN_INVENTORY_SETTING, true))
            .with_setting(Setting::checkbox(HIT_INVISIBLE_SETTING, false))
            .with_setting(Setting::checkbox(ONLY_ARMORED_SETTING, false))
            .with_setting(Setting::checkbox(IGNORE_NAKED_SETTING, false))
            .with_setting(Setting::checkbox(ONLY_X_SETTING, false))
            .with_setting(Setting::checkbox(MULTIPOINT_SETTING, true))
            .with_setting(Setting::checkbox(WALL_CHECK_SETTING, true))
            .with_setting(Setting::checkbox(DISABLE_ON_WORLD_CHANGE_SETTING, false))
            .with_setting(Setting::checkbox(MICRO_MOVEMENTS_SETTING, true))
            .with_setting(Setting::choice(
                ROTATION_MODE_SETTING,
                [MODE_DEFAULT, MODE_NEURO],
                0,
            ))
            .with_setting(Setting::slider(
                MAX_SAMPLES_SETTING,
                20000.0,
                2000.0,
                50000.0,
            ))
            .with_setting(Setting::checkbox(RESET_NEURO_SETTING, false)),
            target_id: None,
            target_acquired_at: 0,
            last_frame_at: None,
            previous_yaw_delta: 0.0,
            previous_pitch_delta: 0.0,
            yaw_phase: 0.0,
            pitch_phase: 0.0,
            yaw_phase_speed: 0.0,
            pitch_phase_speed: 0.0,
            ease_power: 2.5,
            speed_jitter: 1.0,
            aim_height_kick: 0.0,
            ease_variant: 0,
            aim_height_noise: 0.0,
            aim_height_noise_at: now,
            current_point_x: 0.0,
            current_point_y: 0.0,
            current_point_z: 0.0,
            target_point_x: 0.0,
            target_point_y: 0.0,
            target_point_z: 0.0,
            point_update_at: now,
            point_lerp: 0.04,
            dimension_key: None,
            scatter_x: 0.0,
            scatter_y: 0.0,
            scatter_at: now,
            can_aim_at: 0,
            last_target_switch_at: 0,
            neuro: NeuroController::new(),
            last_rotation: None,
            last_attack_clicks: 0,
            last_human_yaw_delta: 0.0,
            last_human_pitch_delta: 0.0,
            prev_prev_human_yaw_delta: 0.0,
            prev_prev_human_pitch_delta: 0.0,
            prev_yaw_error: 0.0,
            prev_pitch_error: 0.0,
            prev_target_pos: None,
            last_attack_at: 0,
            frames_since_acquire: 0.0,
            last_frame_dt: 1.0 / 60.0,
            session_break_pending: false,
            neuro_output: OutputInterpolator::default(),
        };
        module.reseed_humanizer(now);
        module
    }

    fn frame_env(&mut self, env: &mut JNIEnv<'_>, config: &AimAssistConfig) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            self.reset_state();
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            self.reset_state();
            return Ok(());
        }

        if self.handle_world_change(&mut ctx, config)? {
            return Ok(());
        }

        self.sync_neuro_config(config);

        if config.only_with_weapon && !minecraft::player_has_weapon(&mut ctx)? {
            self.publish_overlay(&mut ctx, config, false)?;
            return Ok(());
        }
        if config.no_aim_in_inventory && aim_target::screen_is_container(&mut ctx)? {
            self.publish_overlay(&mut ctx, config, false)?;
            return Ok(());
        }

        let now = time::now_ms();
        let frame_scale = self.frame_scale();
        let target = aim_target::find_best_target(&mut ctx, &config.target_config())?;
        let Some(target) = target else {
            if config.rotation_mode == RotationMode::Neuro {
                let status = self
                    .neuro
                    .tick(NeuroFrameInput {
                        yaw_error: 0.0,
                        pitch_error: 0.0,
                        d_yaw_error: 0.0,
                        d_pitch_error: 0.0,
                        distance: 0.0,
                        rel_x: 0.0,
                        rel_y: 0.0,
                        rel_z: 0.0,
                        target_vel_x: 0.0,
                        target_vel_y: 0.0,
                        target_vel_z: 0.0,
                        view_yaw_degrees: 0.0,
                        half_fov: config.fov as f64 * 0.5,
                        mouse_step: 0.0,
                        dt: self.last_frame_dt,
                        frames_since_acquire: 0.0,
                        time_since_attack: 99.0,
                        attack_held: false,
                        on_ground: true,
                        sprint: false,
                        sneak: false,
                        fall_distance: 0.0,
                        attack_strength: 1.0,
                        target_health: 20.0,
                        prev_yaw_delta: self.last_human_yaw_delta,
                        prev_pitch_delta: self.last_human_pitch_delta,
                        accel_yaw_delta: 0.0,
                        accel_pitch_delta: 0.0,
                        human_d_yaw: 0.0,
                        human_d_pitch: 0.0,
                        session_break: false,
                        has_valid_target: false,
                    })
                    .status_line;
                self.menu.description = status;
                self.neuro.publish_progress_overlay();
            }
            self.reset_tracking(now);
            self.publish_overlay(&mut ctx, config, false)?;
            return Ok(());
        };

        if self.target_id != Some(target.entity_id) {
            
            if now.saturating_sub(self.last_target_switch_at) >= config.switch_cooldown as u64 {
                self.target_id = Some(target.entity_id);
                self.target_acquired_at = now;
                self.last_target_switch_at = now;
                self.can_aim_at = now + config.reaction as u64;
                self.frames_since_acquire = 0.0;
                self.prev_target_pos = None;
                self.prev_yaw_error = 0.0;
                self.prev_pitch_error = 0.0;
                self.session_break_pending = true;
                if config.rotation_mode == RotationMode::Neuro {
                    self.neuro.reset_inference_state();
                    self.neuro_output.reset();
                }
                self.reseed_humanizer(now);
            } else {
                self.publish_overlay(&mut ctx, config, false)?;
                return Ok(());
            }
        }

        self.update_humanizer(config, now);

        
        if now < self.can_aim_at {
            self.publish_overlay(&mut ctx, config, true)?;
            return Ok(());
        }

        self.apply_rotation_policy(&mut ctx, config, target, now, frame_scale)?;
        self.publish_overlay(&mut ctx, config, true)?;
        Ok(())
    }

    fn sync_neuro_config(&mut self, config: &AimAssistConfig) {
        self.neuro.configure(
            config.rotation_mode,
            config.neuro_max_samples as u64,
            config.reset_neuro,
        );
        if config.reset_neuro {
            self.clear_reset_neuro_checkbox();
        }
        self.neuro.publish_progress_overlay();
    }

    fn clear_reset_neuro_checkbox(&mut self) {
        for setting in &mut self.menu.settings {
            if let Setting::CheckBox(box_setting) = setting {
                if box_setting.name == RESET_NEURO_SETTING {
                    box_setting.value = false;
                }
            }
        }
    }

    fn apply_rotation_policy(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
        target: AimTarget,
        now: u64,
        frame_scale: f64,
    ) -> JniResult<()> {
        let current = aim_target::player_rotation(ctx)?;
        let (target_angle, aim_point) = self.neuro_aim_point(ctx, config, target, current)?;
        let yaw_error = rotation::wrap_degrees(target_angle.yaw - current.yaw);
        let pitch_error = target_angle.pitch - current.pitch;

        let dt = self.frame_dt();
        self.frames_since_acquire += dt * 60.0;

        let human_d_yaw = self
            .last_rotation
            .map(|prev| rotation::wrap_degrees(current.yaw - prev.yaw))
            .unwrap_or(0.0);
        let human_d_pitch = self
            .last_rotation
            .map(|prev| current.pitch - prev.pitch)
            .unwrap_or(0.0);
        self.last_rotation = Some(current);

        let d_yaw_error = if dt > 0.0 {
            (yaw_error - self.prev_yaw_error) / dt
        } else {
            0.0
        };
        let d_pitch_error = if dt > 0.0 {
            (pitch_error - self.prev_pitch_error) / dt
        } else {
            0.0
        };
        self.prev_yaw_error = yaw_error;
        self.prev_pitch_error = pitch_error;

        let eye = aim_target::player_eye_position(ctx)?;
        let rel = aim_point.subtract(eye);

        let (target_vel_x, target_vel_y, target_vel_z) = if let Some(prev) = self.prev_target_pos {
            let inv_dt = if dt > 1e-4 { 1.0 / dt } else { 0.0 };
            (
                (target.position.x - prev.x) * inv_dt,
                (target.position.y - prev.y) * inv_dt,
                (target.position.z - prev.z) * inv_dt,
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        self.prev_target_pos = Some(target.position);

        let mouse_step = rotation::mouse_step(ctx).unwrap_or(0.15);
        let on_ground = aim_target::player_on_ground(ctx).unwrap_or(true);
        let attack_edge = self.take_attack_edge(ctx)?;
        if attack_edge {
            self.last_attack_at = now;
        }
        let session_break = attack_edge || self.session_break_pending;
        self.session_break_pending = false;

        let time_since_attack = if self.last_attack_at > 0 {
            (now.saturating_sub(self.last_attack_at)) as f64 / 1000.0
        } else {
            99.0
        };

        let neuro_result = self.neuro.tick(NeuroFrameInput {
            yaw_error,
            pitch_error,
            d_yaw_error,
            d_pitch_error,
            distance: target.distance,
            rel_x: rel.x,
            rel_y: rel.y,
            rel_z: rel.z,
            target_vel_x,
            target_vel_y,
            target_vel_z,
            view_yaw_degrees: current.yaw,
            half_fov: config.fov as f64 * 0.5,
            mouse_step,
            dt,
            frames_since_acquire: self.frames_since_acquire,
            time_since_attack,
            attack_held: self.attack_held(ctx)?,
            on_ground,
            sprint: minecraft::player_is_sprinting(ctx).unwrap_or(false),
            sneak: self.player_is_sneaking(ctx)?,
            fall_distance: aim_target::player_fall_distance(ctx).unwrap_or(0.0),
            attack_strength: minecraft::attack_strength(ctx).unwrap_or(1.0) as f64,
            target_health: target.health,
            prev_yaw_delta: self.last_human_yaw_delta,
            prev_pitch_delta: self.last_human_pitch_delta,
            accel_yaw_delta: self.last_human_yaw_delta - self.prev_prev_human_yaw_delta,
            accel_pitch_delta: self.last_human_pitch_delta - self.prev_prev_human_pitch_delta,
            human_d_yaw,
            human_d_pitch,
            session_break,
            has_valid_target: true,
        });
        self.prev_prev_human_yaw_delta = self.last_human_yaw_delta;
        self.prev_prev_human_pitch_delta = self.last_human_pitch_delta;
        self.last_human_yaw_delta = human_d_yaw;
        self.last_human_pitch_delta = human_d_pitch;

        if config.rotation_mode == RotationMode::Neuro {
            self.menu.description = neuro_result.status_line;
            self.neuro.publish_progress_overlay();
        }

        match neuro_result.authority {
            AimAuthority::ManualOnly => Ok(()),
            AimAuthority::ClassicAssist => self.apply_aim(ctx, config, target, now, frame_scale),
            AimAuthority::NeuroAssist => {
                let Some((pred_yaw, pred_pitch)) = neuro_result.neuro_delta else {
                    return Ok(());
                };
                let error_mag = (yaw_error * yaw_error + pitch_error * pitch_error).sqrt();
                let (pred_yaw, pred_pitch) = blend_with_classic(
                    pred_yaw,
                    pred_pitch,
                    yaw_error,
                    pitch_error,
                    neuro_result.neuro_confidence,
                );
                let (pred_yaw, pred_pitch) =
                    self.neuro_output.step(pred_yaw, pred_pitch, error_mag, dt);

                let (mut yaw_delta, mut pitch_delta) = clamp_delta_toward_error(
                    pred_yaw,
                    pred_pitch,
                    yaw_error,
                    pitch_error,
                    config.deadzone as f64,
                );
                if config.only_x {
                    pitch_delta = 0.0;
                }

                
                let approach = (error_mag / 12.0).clamp(0.18, 1.0);
                let max_step = (1.6 * approach * frame_scale).clamp(0.12, 2.4);
                yaw_delta = yaw_delta.clamp(-max_step, max_step);
                pitch_delta = pitch_delta.clamp(-max_step * 0.8, max_step * 0.8);

                yaw_delta = rotation::snap_delta(yaw_delta, mouse_step);
                pitch_delta = rotation::snap_delta(pitch_delta, mouse_step);
                yaw_delta = clamp_step(yaw_delta, self.previous_yaw_delta, max_step);
                pitch_delta = clamp_step(pitch_delta, self.previous_pitch_delta, max_step * 0.8);
                self.previous_yaw_delta = yaw_delta;
                self.previous_pitch_delta = pitch_delta;

                let settle = if error_mag < config.deadzone as f64 {
                    0.01
                } else if error_mag < 2.5 {
                    0.005
                } else {
                    0.003
                };
                if yaw_delta.abs() < settle && pitch_delta.abs() < settle {
                    return Ok(());
                }
                aim_target::set_player_rotation(
                    ctx,
                    Angle::new(current.yaw + yaw_delta, current.pitch + pitch_delta),
                )
            }
        }
    }

    fn frame_dt(&mut self) -> f64 {
        let now = Instant::now();
        let dt = self
            .last_frame_at
            .map(|last| now.saturating_duration_since(last).as_secs_f64())
            .unwrap_or(1.0 / 60.0)
            .clamp(1.0 / 240.0, 0.2);
        self.last_frame_dt = dt;
        dt
    }

    fn attack_held(&self, ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
        let key = ctx
            .env
            .get_field(
                &ctx.options,
                "keyAttack",
                "Lnet/minecraft/client/KeyMapping;",
            )?
            .l()?;
        if key.is_null() {
            return Ok(false);
        }
        let down = ctx.env.call_method(&key, "isDown", "()Z", &[])?.z()?;
        let _ = ctx.env.delete_local_ref(key);
        Ok(down)
    }

    fn player_is_sneaking(&self, ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
        ctx.env
            .call_method(&ctx.player, "isCrouching", "()Z", &[])?
            .z()
    }

    fn neuro_aim_point(
        &self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
        target: AimTarget,
        current: Angle,
    ) -> JniResult<(Angle, Vec3d)> {
        let eye = aim_target::player_eye_position(ctx)?;
        let mut height = 0.5 + self.aim_height_noise * 2.0;
        if config.micro_movements {
            height += self.aim_height_noise;
        }

        let mut point_offset = Vec3d::default();
        if config.multipoint {
            let half_width = target.width * 0.5;
            point_offset.x = self.current_point_x * half_width;
            point_offset.y = self.current_point_y;
            point_offset.z = self.current_point_z * half_width;
        }

        let height_ratio = (height + point_offset.y).clamp(0.08, 0.92);
        let point = target.position.add(Vec3d {
            x: point_offset.x,
            y: target.height * height_ratio,
            z: point_offset.z,
        });

        let mut angle = rotation::angle_to_point(eye, point, current);
        if config.scatter > 0.0 {
            angle.yaw += self.scatter_x * 0.55;
            angle.pitch += self.scatter_y * 0.55;
        }
        Ok((angle, point))
    }

    fn take_attack_edge(&mut self, ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
        let key = ctx
            .env
            .get_field(
                &ctx.options,
                "keyAttack",
                "Lnet/minecraft/client/KeyMapping;",
            )?
            .l()?;
        if key.is_null() {
            return Ok(false);
        }
        let clicks = ctx.env.get_field(&key, "clickCount", "I")?.i()?;
        let _ = ctx.env.delete_local_ref(key);
        let edge = clicks > self.last_attack_clicks;
        self.last_attack_clicks = clicks;
        Ok(edge)
    }

    fn handle_world_change(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
    ) -> JniResult<bool> {
        let dimension = aim_target::dimension_key(ctx)?;
        match self.dimension_key.as_deref() {
            None => {
                self.dimension_key = Some(dimension);
                Ok(false)
            }
            Some(current) if current == dimension => Ok(false),
            Some(_) if config.disable_on_world_change => {
                self.menu.enabled = false;
                self.reset_state();
                Ok(true)
            }
            Some(_) => {
                self.dimension_key = Some(dimension);
                self.reset_tracking(time::now_ms());
                Ok(true)
            }
        }
    }

    fn frame_scale(&mut self) -> f64 {
        let now = Instant::now();
        let scale = self
            .last_frame_at
            .map(|last| now.saturating_duration_since(last).as_secs_f64() / (1.0 / 60.0))
            .unwrap_or(1.0)
            .clamp(0.05, 3.0);
        self.last_frame_at = Some(now);
        scale
    }

    fn apply_aim(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
        target: AimTarget,
        now: u64,
        frame_scale: f64,
    ) -> JniResult<()> {
        let current = aim_target::player_rotation(ctx)?;
        let target_angle = self.target_angle(ctx, config, target, current)?;
        let yaw_error = rotation::wrap_degrees(target_angle.yaw - current.yaw);
        let pitch_error = target_angle.pitch - current.pitch;
        let pitch_correction = if config.only_x {
            let threshold = if config.micro_movements {
                8.0 + random::range_f64(0.0, 4.0)
            } else {
                10.0
            };
            if pitch_error.abs() > threshold {
                let factor = if config.micro_movements {
                    0.06 + random::range_f64(0.0, 0.06)
                } else {
                    0.08
                };
                (pitch_error - pitch_error.signum() * threshold) * factor
            } else {
                0.0
            }
        } else {
            pitch_error
        };

        let error = if config.only_x {
            yaw_error.abs()
        } else {
            (yaw_error * yaw_error + pitch_correction * pitch_correction).sqrt()
        };
        
        if error < config.deadzone as f64 {
            return Ok(());
        }

        let ramp_ms = if config.micro_movements {
            400.0 + random::range_f64(0.0, 200.0)
        } else {
            350.0
        };
        let ramp = ((now.saturating_sub(self.target_acquired_at)) as f64 / ramp_ms).clamp(0.0, 1.0);
        let ease = self.ease(ramp, config.micro_movements);
        let mut speed = config.speed as f64
            * if config.micro_movements {
                self.speed_jitter
            } else {
                1.0
            };
        if config.micro_movements && random::f64() < 0.015 {
            self.speed_jitter = random::range_f64(0.7, 1.3);
            speed = config.speed as f64 * self.speed_jitter;
        }

        let band = speed_band(error, config.micro_movements);

        let mut aim_speed = speed * ease * band * frame_scale;
        let fall_distance = aim_target::player_fall_distance(ctx).unwrap_or(0.0);
        if fall_distance > 0.0 {
            let fall_factor = if config.micro_movements {
                0.12 + random::range_f64(0.0, 0.08)
            } else {
                0.15
            };
            aim_speed *= 1.0 + (fall_distance * fall_factor).clamp(0.0, 0.7);
        }

        if !aim_target::player_on_ground(ctx).unwrap_or(true)
            && aim_target::player_velocity_y(ctx).unwrap_or(0.0) > 0.1
        {
            aim_speed *= if config.micro_movements {
                0.8 + random::range_f64(0.0, 0.15)
            } else {
                0.85
            };
        }

        let factor_noise = if config.micro_movements {
            random::range_f64(-0.003, 0.003)
        } else {
            0.0
        };
        let factor_cap = if error > 30.0 { 0.6 } else { 0.5 };
        let factor = (aim_speed * 0.028 + factor_noise).clamp(0.005, factor_cap);
        let mut yaw_delta = yaw_error * factor;
        let pitch_scale = if config.micro_movements {
            0.85 + random::range_f64(0.0, 0.3)
        } else {
            1.0
        };
        let mut pitch_delta = pitch_correction * factor * pitch_scale;

        if config.micro_movements && error < 2.5 && self.aim_height_kick > 0.01 {
            let kick = self.aim_height_kick * random::range_f64(0.4, 1.0);
            yaw_delta *= 1.0 + kick;
            pitch_delta *= 1.0 + kick * 0.7;
            self.aim_height_kick *= 0.9 + random::range_f64(0.0, 0.07);
        }

        if config.micro_movements {
            let noise = 0.18 + random::range_f64(0.0, 0.28);
            self.yaw_phase += self.yaw_phase_speed + random::range_f64(-0.03, 0.03);
            self.pitch_phase += self.pitch_phase_speed + random::range_f64(-0.025, 0.025);
            yaw_delta += self.yaw_phase.sin() * noise * (0.5 + random::range_f64(0.0, 0.8));
            pitch_delta +=
                self.pitch_phase.cos() * noise * 0.55 * (0.4 + random::range_f64(0.0, 0.7));
        }

        let max_yaw_step = if config.micro_movements {
            2.0 + random::range_f64(0.0, 1.5)
        } else {
            3.0
        };
        let max_pitch_step = if config.micro_movements {
            1.5 + random::range_f64(0.0, 1.0)
        } else {
            2.5
        };
        yaw_delta = clamp_step(yaw_delta, self.previous_yaw_delta, max_yaw_step);
        pitch_delta = clamp_step(pitch_delta, self.previous_pitch_delta, max_pitch_step);
        self.previous_yaw_delta = yaw_delta;
        self.previous_pitch_delta = pitch_delta;

        if config.micro_movements {
            if let Ok(step) = rotation::mouse_step(ctx) {
                yaw_delta = rotation::snap_delta(yaw_delta, step);
                pitch_delta = rotation::snap_delta(pitch_delta, step);
            }
        }

        if yaw_delta.abs() < 0.003 && pitch_delta.abs() < 0.003 {
            return Ok(());
        }

        aim_target::set_player_rotation(
            ctx,
            Angle::new(current.yaw + yaw_delta, current.pitch + pitch_delta),
        )
    }

    fn target_angle(
        &self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
        target: AimTarget,
        current: Angle,
    ) -> JniResult<Angle> {
        let eye = aim_target::player_eye_position(ctx)?;
        
        let mut height = 0.5 + self.aim_height_noise * 2.0;
        if config.micro_movements {
            height += self.aim_height_noise;
        }

        let mut point_offset = Vec3d::default();
        if config.multipoint {
            let half_width = target.width * 0.5;
            point_offset.x = self.current_point_x * half_width;
            point_offset.y = self.current_point_y;
            point_offset.z = self.current_point_z * half_width;
        }

        let height_ratio = (height + point_offset.y).clamp(0.05, 0.95);
        let point = target.position.add(Vec3d {
            x: point_offset.x,
            y: target.height * height_ratio,
            z: point_offset.z,
        });

        let mut angle = rotation::angle_to_point(eye, point, current);

        
        if config.scatter > 0.0 {
            angle.yaw += self.scatter_x;
            angle.pitch += self.scatter_y;
        }

        Ok(angle)
    }

    fn update_humanizer(&mut self, config: &AimAssistConfig, now: u64) {
        if config.micro_movements
            && now.saturating_sub(self.aim_height_noise_at) > random::range_u64_between(250, 600)
        {
            
            self.aim_height_noise =
                (self.aim_height_noise + random::range_f64(-0.12, 0.12)).clamp(-0.3, 0.3);
            self.aim_height_noise_at = now;
        }

        if config.multipoint && now >= self.point_update_at {
            self.reseed_multipoint(now);
        }

        
        if config.scatter > 0.0 && now.saturating_sub(self.scatter_at) > 250 {
            let s = config.scatter as f64;
            self.scatter_x = random::range_f64(-s, s);
            self.scatter_y = random::range_f64(-s, s) * 0.6;
            self.scatter_at = now;
        }

        if config.multipoint {
            let lerp = self.point_lerp
                + if config.micro_movements {
                    random::range_f64(-0.008, 0.008)
                } else {
                    0.0
                };
            self.current_point_x += (self.target_point_x - self.current_point_x) * lerp;
            self.current_point_y += (self.target_point_y - self.current_point_y) * lerp;
            self.current_point_z += (self.target_point_z - self.current_point_z) * lerp;
        }
    }

    fn ease(&self, value: f64, micro: bool) -> f64 {
        let value = value.clamp(0.0, 1.0);
        if !micro {
            return 1.0 - (1.0 - value).powf(2.5);
        }

        let eased = match self.ease_variant {
            0 => 1.0 - (1.0 - value).powf(self.ease_power),
            1 if value < 0.5 => (2.0 * value).powf(self.ease_power) / 2.0,
            1 => 1.0 - (2.0 * (1.0 - value)).powf(self.ease_power) / 2.0,
            _ => value * value * (3.0 - 2.0 * value),
        } * (0.85 + random::range_f64(0.0, 0.3));

        eased.clamp(0.05, 1.8)
    }

    fn reseed_humanizer(&mut self, now: u64) {
        self.yaw_phase = random::range_f64(0.0, TAU);
        self.pitch_phase = random::range_f64(0.0, TAU);
        self.yaw_phase_speed = random::range_f64(0.15, 0.45);
        self.pitch_phase_speed = random::range_f64(0.12, 0.38);
        self.ease_power = random::range_f64(1.8, 3.5);
        self.speed_jitter = random::range_f64(0.75, 1.25);
        self.aim_height_kick = random::range_f64(-0.1, 0.2);
        self.ease_variant = random::range_u64(3) as i32;
        self.aim_height_noise = random::range_f64(-0.15, 0.1);
        self.aim_height_noise_at = now;
        self.point_lerp = random::range_f64(0.025, 0.065);
        self.reseed_multipoint(now);
    }

    fn reseed_multipoint(&mut self, now: u64) {
        self.target_point_x = random::range_f64(-0.35, 0.35);
        self.target_point_y = random::range_f64(-0.12, 0.12);
        self.target_point_z = random::range_f64(-0.35, 0.35);
        self.point_update_at = now + random::range_u64_between(300, 900);
        self.point_lerp = random::range_f64(0.02, 0.07);
    }

    fn reset_tracking(&mut self, now: u64) {
        self.target_id = None;
        self.target_acquired_at = 0;
        self.last_frame_at = None;
        self.previous_yaw_delta = 0.0;
        self.previous_pitch_delta = 0.0;
        self.current_point_x = 0.0;
        self.current_point_y = 0.0;
        self.current_point_z = 0.0;
        self.scatter_x = 0.0;
        self.scatter_y = 0.0;
        self.can_aim_at = 0;
        self.last_target_switch_at = 0;
        self.prev_yaw_error = 0.0;
        self.prev_pitch_error = 0.0;
        self.prev_target_pos = None;
        self.frames_since_acquire = 0.0;
        self.session_break_pending = false;
        self.neuro.reset_inference_state();
        self.neuro_output.reset();
        self.reseed_humanizer(now);
    }

    fn reset_state(&mut self) {
        self.dimension_key = None;
        self.last_rotation = None;
        self.last_attack_clicks = 0;
        self.last_human_yaw_delta = 0.0;
        self.last_human_pitch_delta = 0.0;
        self.reset_tracking(time::now_ms());
        aim_target::clear_overlay_state();
        crate::utils::neuro::clear_progress_overlay();
    }

    fn publish_overlay(
        &self,
        ctx: &mut MinecraftContext<'_, '_>,
        config: &AimAssistConfig,
        target_active: bool,
    ) -> JniResult<()> {
        let visible = config.draw_fov_radius && aim_target::screen_is_none(ctx)?;
        let vanilla_fov = aim_target::vanilla_fov(ctx).unwrap_or(90.0);
        aim_target::set_overlay_state(AimAssistOverlay {
            visible,
            target_active,
            target_entity_id: if target_active { self.target_id } else { None },
            aim_fov_degrees: config.fov,
            vanilla_fov_degrees: vanilla_fov,
        });
        Ok(())
    }
}

impl ClientModule for AimAssistModule {
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
        crate::utils::neuro::clear_progress_overlay();
    }

    fn on_frame(&mut self) {
        let config = AimAssistConfig::from_module(&self.menu);
        self.neuro.configure(
            config.rotation_mode,
            config.neuro_max_samples as u64,
            config.reset_neuro,
        );
        if config.reset_neuro {
            self.clear_reset_neuro_checkbox();
        }
        self.neuro.publish_progress_overlay();

        let _ = jni_bridge::with_env(|env| {
            let result = self.frame_env(env, &config);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
                self.reset_state();
            }
            result
        });
    }
}

#[derive(Clone)]
struct AimAssistConfig {
    distance: f32,
    fov: f32,
    draw_fov_radius: bool,
    speed: f32,
    priority: AimPriority,
    only_with_weapon: bool,
    only_players: bool,
    no_aim_in_inventory: bool,
    hit_invisible: bool,
    only_armored: bool,
    ignore_naked: bool,
    only_x: bool,
    multipoint: bool,
    wall_check: bool,
    disable_on_world_change: bool,
    micro_movements: bool,
    deadzone: f32,
    scatter: f32,
    reaction: f32,
    switch_cooldown: f32,
    rotation_mode: RotationMode,
    neuro_max_samples: f32,
    reset_neuro: bool,
}

impl AimAssistConfig {
    fn from_module(module: &Module) -> Self {
        let priority = settings::choice(module, PRIORITY_SETTING, PRIORITY_DISTANCE);
        let rotation_mode = RotationMode::from_choice(&settings::choice(
            module,
            ROTATION_MODE_SETTING,
            MODE_DEFAULT,
        ));
        Self {
            distance: settings::slider(module, DISTANCE_SETTING, 4.0),
            fov: settings::slider(module, FOV_SETTING, 90.0),
            draw_fov_radius: settings::checkbox(module, DRAW_FOV_SETTING, false),
            speed: settings::slider(module, SPEED_SETTING, 4.0),
            priority: if priority == PRIORITY_HEALTH {
                AimPriority::Health
            } else {
                AimPriority::Distance
            },
            only_with_weapon: settings::checkbox(module, ONLY_WITH_WEAPON_SETTING, true),
            only_players: settings::checkbox(module, ONLY_PLAYERS_SETTING, true),
            no_aim_in_inventory: settings::checkbox(module, NO_AIM_IN_INVENTORY_SETTING, true),
            hit_invisible: settings::checkbox(module, HIT_INVISIBLE_SETTING, false),
            only_armored: settings::checkbox(module, ONLY_ARMORED_SETTING, false),
            ignore_naked: settings::checkbox(module, IGNORE_NAKED_SETTING, false),
            only_x: settings::checkbox(module, ONLY_X_SETTING, false),
            multipoint: settings::checkbox(module, MULTIPOINT_SETTING, true),
            wall_check: settings::checkbox(module, WALL_CHECK_SETTING, true),
            disable_on_world_change: settings::checkbox(
                module,
                DISABLE_ON_WORLD_CHANGE_SETTING,
                false,
            ),
            micro_movements: settings::checkbox(module, MICRO_MOVEMENTS_SETTING, true),
            deadzone: 1.2,
            scatter: 1.5,
            reaction: 120.0,
            switch_cooldown: 250.0,
            rotation_mode,
            neuro_max_samples: settings::slider(module, MAX_SAMPLES_SETTING, 20000.0),
            reset_neuro: settings::checkbox(module, RESET_NEURO_SETTING, false),
        }
    }

    fn target_config(&self) -> AimTargetConfig {
        AimTargetConfig {
            distance: self.distance as f64,
            fov_degrees: self.fov as f64,
            aim_height: 0.5,
            priority: self.priority,
            only_players: self.only_players,
            hit_invisible: self.hit_invisible,
            only_armored: self.only_armored,
            ignore_naked: self.ignore_naked,
            wall_check: self.wall_check,
        }
    }
}

fn speed_band(error: f64, micro: bool) -> f64 {
    if micro {
        if error < 3.0 {
            0.35 + random::range_f64(0.0, 0.25)
        } else if error < 10.0 {
            0.6 + random::range_f64(0.0, 0.25)
        } else if error < 25.0 {
            0.75 + random::range_f64(0.0, 0.25)
        } else if error < 50.0 {
            0.9 + random::range_f64(0.0, 0.25)
        } else {
            1.0 + random::range_f64(0.0, 0.2)
        }
    } else if error < 3.0 {
        0.45
    } else if error < 10.0 {
        0.7
    } else if error < 25.0 {
        0.85
    } else if error < 50.0 {
        0.95
    } else {
        1.05
    }
}

fn clamp_step(value: f64, previous: f64, max_step: f64) -> f64 {
    let delta = value - previous;
    if delta.abs() > max_step {
        previous + delta.signum() * max_step
    } else {
        value
    }
}
