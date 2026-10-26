



use std::time::Instant;

use jni::{errors::Result as JniResult, objects::JObject, objects::JValue};

use crate::{
    interface::{Category, Module, Setting},
    jni_bridge,
    module_system::ClientModule,
    utils::{
        aim_target::{self, AimAssistOverlay},
        aura::{
            best_point::BestPoint, ideal_hit::IdealHit, multipoint::Multipoint,
            predict::Predict, ray_trace, rotation::Rotation, rotation_storage,
            rotation_utils::RotationUtils, timer::TimerUtils,
        },
        friends,
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        neuro::{AimAuthority, NeuroController, NeuroFrameInput, RotationMode},
        random, rotation, settings, time,
    },
};

const ROTATION_TYPE: &str = "Ротация";
const RANGE: &str = "Дистанция атаки";
const AIM_RANGE: &str = "Дистанция наводки";
const ELYTRA_AIM_RANGE: &str = "Дистанция на элитрах";
const SMART_CRIT: &str = "Умные криты";
const SPRINT_RESET: &str = "Сброс спринта";
const THROUGH_WALLS: &str = "Бить через стены";
const RAYCAST: &str = "Проверка на наведение";
const UNPRESS_SHIELD: &str = "Отжимать щит";
const BREAK_SHIELD: &str = "Ломать щит";
const ATTACK_ON_EATING: &str = "Не бить когда ешь";
const CLIENT_LOOK: &str = "РќР°РІРѕРґРєР° РѕС‚ РїРµСЂРІРѕРіРѕ Р»РёС†Р°";
const MOVE_FIX: &str = "РљРѕСЂСЂРµРєС†РёСЏ";
const PRIORITY: &str = "РџСЂРёРѕСЂРёС‚РµС‚";
const RW_WALL_BYPASS: &str = "Wall Bypass";
const RW_WALL_LOOK_DOWN: &str = "RW Bypass";
const T_PLAYERS: &str = "Players";
const T_NAKED: &str = "Naked";
const T_INVIS: &str = "Invisible";
const T_PASSIVE: &str = "Passive";
const T_MOBS: &str = "Mobs";

const ROT_SMOOTH: &str = "Smooth";
const ROT_SNAP: &str = "Snap";
const ROT_DATA: &str = "Data";
const ROT_SLOTH: &str = "Sloth";
const ROT_NO_ROTATE: &str = "NoRotate";

const FIX_NONE: &str = "None";
const FIX_FREE: &str = "Free";
const FIX_FOCUSED: &str = "Focused";
const FIX_FULL: &str = "Full";

const PRIO_DISTANCE: &str = "Distance";
const PRIO_HEALTH: &str = "Health";
const PRIO_ANGLE: &str = "Health";
const PRIO_NONE: &str = "None";


const ATTACK_COOLDOWN_MS: u64 = 467;
const SNAP_YAW_TOLERANCE: f32 = 3.0;
const SNAP_PITCH_TOLERANCE: f32 = 2.5;
const DATA_YAW_TOLERANCE: f64 = 1.15;
const DATA_PITCH_TOLERANCE: f64 = 0.9;

pub struct AttackAuraModule {
    menu: Module,
    target_id: Option<i32>,
    
    target_rotations: Rotation,
    
    current_rotations: Rotation,
    attack_timer: TimerUtils,
    back_timer: TimerUtils,
    
    cps_deadline_ms: u64,
    need_sprint_reset: bool,
    sprint_reset_done: bool,
    sprint_reset_ticks: i32,
    ticks_to_attack: i32,
    snap_attack_age: i32,
    snap_attack_queued: bool,
    snap_attack_target: Option<i32>,
    last_data_target: Option<i32>,
    
    rotation_point: Vec3d,
    rotation_motion: Vec3d,
    
    neuro: NeuroController,
    last_frame_at: Option<Instant>,
    last_human_yaw_delta: f64,
    last_human_pitch_delta: f64,
    prev_yaw_error: f64,
    prev_pitch_error: f64,
    prev_target_pos: Option<Vec3d>,
    last_attack_at: u64,
}

impl AttackAuraModule {
    pub fn new() -> Self {
        let menu = Module::new(
            "AttackAura",
            "Wonderful Aura: Р°РІС‚Рѕ-РЅР°РІРѕРґРєР° Рё Р°С‚Р°РєР° (СЂРѕС‚Р°С†РёРё, РјСѓР»СЊС‚РёРїРѕРёРЅС‚, С€РёР»РґР±СЂРµР№Рє, РєСЂРёС‚С‹)",
            Category::Combat,
        )
        .with_setting(Setting::choice(
            ROTATION_TYPE,
            [ROT_SMOOTH, ROT_SNAP, ROT_DATA, ROT_SLOTH, ROT_NO_ROTATE],
            0,
        ))
        .with_setting(Setting::slider(RANGE, 3.0, 0.0, 6.0))
        .with_setting(Setting::slider(AIM_RANGE, 3.0, 0.0, 6.0))
        .with_setting(Setting::slider(ELYTRA_AIM_RANGE, 50.0, 10.0, 100.0))
        .with_setting(Setting::checkbox(SMART_CRIT, false))
        .with_setting(Setting::checkbox(SPRINT_RESET, true))
        .with_setting(Setting::checkbox(THROUGH_WALLS, true))
        .with_setting(Setting::checkbox(RAYCAST, false))
        .with_setting(Setting::checkbox(UNPRESS_SHIELD, false))
        .with_setting(Setting::checkbox(BREAK_SHIELD, true))
        .with_setting(Setting::checkbox(ATTACK_ON_EATING, true))
        .with_setting(Setting::checkbox(CLIENT_LOOK, false))
        .with_setting(Setting::choice(
            MOVE_FIX,
            [FIX_NONE, FIX_FREE, FIX_FOCUSED, FIX_FULL],
            0,
        ))
        .with_setting(Setting::choice(
            PRIORITY,
            [PRIO_DISTANCE, PRIO_HEALTH, PRIO_ANGLE, PRIO_NONE],
            0,
        ))
        .with_setting(Setting::checkbox(RW_WALL_BYPASS, false))
        .with_setting(Setting::checkbox(RW_WALL_LOOK_DOWN, false))
        .with_setting(Setting::checkbox(T_PLAYERS, true))
        .with_setting(Setting::checkbox(T_NAKED, true))
        .with_setting(Setting::checkbox(T_INVIS, true))
        .with_setting(Setting::checkbox(T_PASSIVE, false))
        .with_setting(Setting::checkbox(T_MOBS, true));
        Self {
            menu,
            target_id: None,
            target_rotations: Rotation::default(),
            current_rotations: Rotation::default(),
            attack_timer: TimerUtils::new(),
            back_timer: TimerUtils::new(),
            cps_deadline_ms: 0,
            need_sprint_reset: false,
            sprint_reset_done: false,
            sprint_reset_ticks: 0,
            ticks_to_attack: 0,
            snap_attack_age: -1,
            snap_attack_queued: false,
            snap_attack_target: None,
            last_data_target: None,
            rotation_point: Vec3d::default(),
            rotation_motion: Vec3d::default(),
            neuro: NeuroController::new(),
            last_frame_at: None,
            last_human_yaw_delta: 0.0,
            last_human_pitch_delta: 0.0,
            prev_yaw_error: 0.0,
            prev_pitch_error: 0.0,
            prev_target_pos: None,
            last_attack_at: 0,
        }
    }

    fn cfg(&self) -> Cfg {
        Cfg {
            rotation_type: settings::choice(&self.menu, ROTATION_TYPE, ROT_SMOOTH),
            range: settings::slider(&self.menu, RANGE, 3.0),
            aim_range: settings::slider(&self.menu, AIM_RANGE, 3.0),
            elytra_aim_range: settings::slider(&self.menu, ELYTRA_AIM_RANGE, 50.0),
            smart_crit: settings::checkbox(&self.menu, SMART_CRIT, false),
            sprint_reset: settings::checkbox(&self.menu, SPRINT_RESET, true),
            through_walls: settings::checkbox(&self.menu, THROUGH_WALLS, true),
            raycast: settings::checkbox(&self.menu, RAYCAST, false),
            unpress_shield: settings::checkbox(&self.menu, UNPRESS_SHIELD, false),
            break_shield: settings::checkbox(&self.menu, BREAK_SHIELD, true),
            attack_on_eating: settings::checkbox(&self.menu, ATTACK_ON_EATING, true),
            client_look: settings::checkbox(&self.menu, CLIENT_LOOK, false),
            move_fix: settings::choice(&self.menu, MOVE_FIX, FIX_NONE),
            priority: settings::choice(&self.menu, PRIORITY, PRIO_DISTANCE),
            rw_bypass: settings::checkbox(&self.menu, RW_WALL_BYPASS, false),
            rw_look_down: settings::checkbox(&self.menu, RW_WALL_LOOK_DOWN, false),
            t_players: settings::checkbox(&self.menu, T_PLAYERS, true),
            t_naked: settings::checkbox(&self.menu, T_NAKED, true),
            t_invis: settings::checkbox(&self.menu, T_INVIS, true),
            t_passive: settings::checkbox(&self.menu, T_PASSIVE, false),
            t_mobs: settings::checkbox(&self.menu, T_MOBS, true),
        }
    }

    fn reset_state(&mut self) {
        self.target_id = None;
        self.cps_deadline_ms = 0;
        self.back_timer.reset();
        self.rotation_point = Vec3d::default();
        self.rotation_motion = Vec3d::default();
        self.last_data_target = None;
        self.need_sprint_reset = false;
        self.sprint_reset_done = false;
        self.sprint_reset_ticks = 0;
        self.ticks_to_attack = 0;
        self.reset_snap_attack();
        self.prev_target_pos = None;
        self.prev_yaw_error = 0.0;
        self.prev_pitch_error = 0.0;
        rotation_storage::clear();
        aim_target::clear_overlay_state();
    }

    fn reset_snap_attack(&mut self) {
        self.snap_attack_age = -1;
        self.snap_attack_queued = false;
        self.snap_attack_target = None;
    }

    
    fn is_snap_rotation_active(&self, cfg: &Cfg) -> bool {
        cfg.rotation_type == ROT_SNAP
    }
}

struct Cfg {
    rotation_type: String,
    range: f32,
    aim_range: f32,
    elytra_aim_range: f32,
    smart_crit: bool,
    sprint_reset: bool,
    through_walls: bool,
    raycast: bool,
    unpress_shield: bool,
    break_shield: bool,
    attack_on_eating: bool,
    client_look: bool,
    move_fix: String,
    priority: String,
    rw_bypass: bool,
    rw_look_down: bool,
    t_players: bool,
    t_naked: bool,
    t_invis: bool,
    t_passive: bool,
    t_mobs: bool,
}


impl ClientModule for AttackAuraModule {
    fn menu_module(&self) -> &Module {
        &self.menu
    }

    fn menu_module_mut(&mut self) -> &mut Module {
        &mut self.menu
    }

    fn on_enable(&mut self) {
        self.reset_state();
        if let Some(rot) = jni_bridge::with_env(|env| {
            let Some(mut ctx) = MinecraftContext::new(env)? else {
                return Ok(None);
            };
            aim_target::player_rotation(&mut ctx).map(Some)
        })
        .unwrap_or_default()
        {
            self.current_rotations = Rotation::new(rot.yaw as f32, rot.pitch as f32);
        }
    }

    fn on_disable(&mut self) {
        self.reset_state();
    }

    
    fn on_tick(&mut self) {
        if !self.menu.enabled {
            return;
        }
        if self.ticks_to_attack > 0 {
            self.ticks_to_attack -= 1;
        }
        if self.sprint_reset_done {
            self.sprint_reset_ticks += 1;
        }
        let cfg = self.cfg();
        let _ = jni_bridge::with_env(|env| {
            let result = self.tick_env(env, &cfg);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }

    
    fn on_frame(&mut self) {
        if !self.menu.enabled {
            return;
        }
        let cfg = self.cfg();
        let _ = jni_bridge::with_env(|env| {
            let result = self.frame_env(env, &cfg);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });
    }
}

impl AttackAuraModule {
    fn tick_env(&mut self, env: &mut jni::JNIEnv<'_>, cfg: &Cfg) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(());
        }
        self.process_attack(&mut ctx, cfg)
    }

    fn frame_env(&mut self, env: &mut jni::JNIEnv<'_>, cfg: &Cfg) -> JniResult<()> {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(());
        }
        if self.target_id.is_some() {
            self.rotate(&mut ctx, cfg)?;
        }
        Ok(())
    }

    
    fn process_attack(&mut self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> JniResult<()> {
        self.update_target(ctx, cfg)?;
        let now = time::now_ms();

        let Some(id) = self.target_id else {
            
            self.cps_deadline_ms = now;
            self.back_timer.reset();
            self.rotation_point = Vec3d::default();
            self.rotation_motion = Vec3d::default();
            self.last_data_target = None;
            self.sprint_reset_done = false;
            self.sprint_reset_ticks = 0;
            self.ticks_to_attack = 0;
            self.reset_snap_attack();
            return Ok(());
        };

        let snap_queued_ready = self.should_use_queued_snap_attack(ctx, id)?;
        if !(self.should_attack(ctx, cfg, id)? || snap_queued_ready) || now < self.cps_deadline_ms {
            return Ok(());
        }

        
        if cfg.attack_on_eating && is_using_item(ctx)? {
            return Ok(());
        }

        
        if cfg.sprint_reset && is_sprinting(ctx)? && !self.sprint_reset_done {
            self.need_sprint_reset = true;
            zero_move_input(ctx)?;
            self.need_sprint_reset = false;
            self.sprint_reset_done = true;
            self.sprint_reset_ticks = 0;
            return Ok(());
        }
        if cfg.sprint_reset && self.sprint_reset_done && self.sprint_reset_ticks < 1 {
            return Ok(());
        }

        if self.is_snap_rotation_active(cfg) && !self.prepare_snap_attack(ctx, cfg, id)? {
            return Ok(());
        }

        self.attack(ctx, cfg, id)?;
        self.reset_snap_attack();
        self.sprint_reset_done = false;
        self.sprint_reset_ticks = 0;
        Ok(())
    }

    
    fn update_target(&mut self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> JniResult<()> {
        if let Some(id) = self.target_id {
            if let Some(entity) = ctx.entity_by_id(id)? {
                let valid = is_valid_target(ctx, &entity, cfg)?;
                let _ = ctx.env.delete_local_ref(entity);
                if valid {
                    return Ok(());
                }
            }
        }
        self.target_id = self.find_target(ctx, cfg)?;
        Ok(())
    }

    
    fn find_target(&self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> JniResult<Option<i32>> {
        let iterable = player_iterable(ctx.env, &ctx.level)?;
        let iter = ctx
            .env
            .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
            .l()?;
        let eye = aim_target::player_eye_position(ctx)?;
        let max_aim = max_aim_range(ctx, cfg);
        let player_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
        let mut candidates: Vec<(i32, f64)> = Vec::new();

        while ctx.env.call_method(&iter, "hasNext", "()Z", &[])?.z()? {
            let e = ctx
                .env
                .call_method(&iter, "next", "()Ljava/lang/Object;", &[])?
                .l()?;
            let valid = is_valid_target(ctx, &e, cfg)?;
            let mut score = f64::INFINITY;
            if valid {
                let (bmin, bmax) = get_aabb(ctx, &e)?;
                let nearest = BestPoint::get_nearest_point(bmin, bmax, eye);
                if eye.distance_to(nearest) <= max_aim
                    && (cfg.through_walls || cfg.rw_bypass || entity_can_see(ctx, &e)?)
                {
                    let id = ctx.env.call_method(&e, "getId", "()I", &[])?.i()?;
                    score = match cfg.priority.as_str() {
                        PRIO_HEALTH => {
                            ctx.env.call_method(&e, "getHealth", "()F", &[])?.f().unwrap_or(20.0) as f64
                        }
                        PRIO_ANGLE => {
                            let center = Vec3d {
                                x: (bmin.x + bmax.x) * 0.5,
                                y: (bmin.y + bmax.y) * 0.5,
                                z: (bmin.z + bmax.z) * 0.5,
                            };
                            let (yaw, pitch) = RotationUtils::get_rotations(eye, center);
                            let cur = aim_target::player_rotation(ctx)?;
                            (rotation::wrap_degrees((yaw - cur.yaw as f32) as f64).abs()
                                + (pitch - cur.pitch as f32).abs() as f64)
                        }
                        PRIO_NONE => 0.0,
                        _ => {
                            let center = Vec3d {
                                x: (bmin.x + bmax.x) * 0.5,
                                y: (bmin.y + bmax.y) * 0.5,
                                z: (bmin.z + bmax.z) * 0.5,
                            };
                            ((eye.x - center.x).powi(2) + (eye.y - center.y).powi(2) + (eye.z - center.z).powi(2))
                        }
                    };
                    candidates.push((id, score));
                }
            }
            let _ = ctx.env.delete_local_ref(e);
        }
        let _ = ctx.env.delete_local_ref(iter);
        let _ = ctx.env.delete_local_ref(iterable);
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(candidates.first().map(|c| c.0))
    }
}


impl AttackAuraModule {
    
    fn should_attack(&self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg, target_id: i32) -> JniResult<bool> {
        
        let cooldown = attack_strength_scale(ctx)?;
        if cooldown < IdealHit::get_ai_cooldown(ctx) {
            return Ok(false);
        }

        let eye = aim_target::player_eye_position(ctx)?;
        let look = view_vector(ctx)?;
        let reach = (cfg.range * 2.0) as f64;

        
        if cfg.raycast {
            if let Some(target) = ctx.entity_by_id(target_id)? {
                let (bmin, bmax) = get_aabb(ctx, &target)?;
                let _ = ctx.env.delete_local_ref(target);
                if !ray_trace_hit(eye, look, reach, bmin, bmax) {
                    return Ok(false);
                }
            }
        }

        
        if cfg.rotation_type == ROT_DATA {
            let (tyaw, tpitch) = self.data_target_rotation(ctx, cfg, target_id)?;
            let cur = aim_target::player_rotation(ctx)?;
            let yaw_diff = rotation::wrap_degrees((tyaw - cur.yaw as f32) as f64).abs();
            let pitch_diff = (tpitch - cur.pitch as f32).abs();
            let on_target = entity_ray_hit(ctx, target_id, eye, look, reach)?;
            if !((yaw_diff <= DATA_YAW_TOLERANCE as f64 && (pitch_diff as f64) <= DATA_PITCH_TOLERANCE as f64) && on_target) {
                return Ok(false);
            }
        }

        
        if let Some(target) = ctx.entity_by_id(target_id)? {
            let (bmin, bmax) = get_aabb(ctx, &target)?;
            let _ = ctx.env.delete_local_ref(target);
            let center = Vec3d {
                x: (bmin.x + bmax.x) * 0.5,
                y: (bmin.y + bmax.y) * 0.5,
                z: (bmin.z + bmax.z) * 0.5,
            };
            let check_point = if eye.distance_to(center) > 3.0 {
                BestPoint::get_nearest_point(bmin, bmax, eye)
            } else {
                center
            };
            if eye.distance_to(check_point) > cfg.range as f64 {
                return Ok(false);
            }
        }

        
        Ok(self.can_crit(ctx, cfg))
    }

    
    fn can_crit(&self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> bool {
        let on_ground = aim_target::player_on_ground(ctx).unwrap_or(true);
        let vel_y = aim_target::player_velocity_y(ctx).unwrap_or(0.0);
        let fall = aim_target::player_fall_distance(ctx).unwrap_or(0.0);
        let crit_possible = !on_ground && vel_y < 0.0 && fall > 0.0;
        if cfg.smart_crit {
            
            on_ground || crit_possible
        } else {
            crit_possible
        }
    }

    
    fn attack(&mut self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg, target_id: i32) -> JniResult<()> {
        let Some(target) = ctx.entity_by_id(target_id)? else {
            return Ok(());
        };

        
        let mut shield_hand: Option<jni::objects::JObject<'_>> = None;
        if cfg.unpress_shield {
            shield_hand = get_blocking_shield_hand(ctx.env, &ctx.player)?;
            if let Some(hand) = &shield_hand {
                let _ = ctx.env.call_method(
                    &ctx.player,
                    "stopUsingItem",
                    "()V",
                    &[],
                );
                jni_bridge::clear_pending_exception(ctx.env);
            }
        }

        let blocking = ctx.env.call_method(&target, "isBlocking", "()Z", &[])?.z().unwrap_or(false);
        let mut attacked = false;
        if blocking && cfg.break_shield {
            attacked = self.shield_break(ctx, &target)?;
        }
        if !attacked {
            ctx.env.call_method(
                &ctx.game_mode,
                "attack",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/Entity;)V",
                &[JValue::Object(&ctx.player), JValue::Object(&target)],
            )?;
        }
        let hand_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.InteractionHand")?;
        let mh = ctx
            .env
            .get_static_field(hand_cls, "MAIN_HAND", "Lnet/minecraft/world/InteractionHand;")?
            .l()?;
        ctx.env.call_method(
            &ctx.player,
            "swing",
            "(Lnet/minecraft/world/InteractionHand;)V",
            &[JValue::Object(&mh)],
        )?;
        let _ = ctx.env.delete_local_ref(mh);

        
        if let Some(hand) = shield_hand {
            let _ = ctx.env.call_method(
                &ctx.game_mode,
                "useItem",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;",
                &[JValue::Object(&ctx.player), JValue::Object(&hand)],
            );
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(hand);
        }

        self.cps_deadline_ms = time::now_ms() + ATTACK_COOLDOWN_MS;
        self.ticks_to_attack = 10;
        self.attack_timer.reset();
        self.last_attack_at = time::now_ms();
        let _ = ctx.env.delete_local_ref(target);
        Ok(())
    }

    
    fn shield_break(&mut self, ctx: &mut MinecraftContext<'_, '_>, target: &JObject<'_>) -> JniResult<bool> {
        let Some(axe_slot) = find_axe_slot(ctx)? else {
            return Ok(false);
        };
        let inv = ctx
            .env
            .call_method(&ctx.player, "getInventory", "()Lnet/minecraft/world/entity/player/Inventory;", &[])?
            .l()?;
        let previous_slot = ctx
            .env
            .call_method(&inv, "getSelectedSlot", "()I", &[])?
            .i()
            .unwrap_or(0);
        let _ = ctx.env.delete_local_ref(inv);

        if axe_slot == previous_slot {
            ctx.env.call_method(
                &ctx.game_mode,
                "attack",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/Entity;)V",
                &[JValue::Object(&ctx.player), JValue::Object(target)],
            )?;
            return Ok(true);
        }
        
        
        let inv = ctx
            .env
            .call_method(&ctx.player, "getInventory", "()Lnet/minecraft/world/entity/player/Inventory;", &[])?
            .l()?;
        let _ = ctx.env.call_method(&inv, "setSelectedSlot", "(I)V", &[JValue::Int(axe_slot)])?;
        let _ = ctx.env.delete_local_ref(inv);
        let attack_result: JniResult<()> = (|| {
            ctx.env.call_method(
                &ctx.game_mode,
                "attack",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/Entity;)V",
                &[JValue::Object(&ctx.player), JValue::Object(target)],
            )?;
            Ok(())
        })();
        let inv = ctx
            .env
            .call_method(&ctx.player, "getInventory", "()Lnet/minecraft/world/entity/player/Inventory;", &[])?
            .l()?;
        let _ = ctx.env.call_method(&inv, "setSelectedSlot", "(I)V", &[JValue::Int(previous_slot)])?;
        let _ = ctx.env.delete_local_ref(inv);
        attack_result?;
        Ok(true)
    }
}


impl AttackAuraModule {
    
    fn rotate(&mut self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> JniResult<()> {
        let Some(id) = self.target_id else {
            return Ok(());
        };
        let Some(target) = ctx.entity_by_id(id)? else {
            return Ok(());
        };

        
        if cfg.rotation_type == ROT_DATA && self.last_data_target != Some(id) {
            self.rotation_point = Vec3d::default();
            self.rotation_motion = Vec3d::default();
            self.last_data_target = Some(id);
            self.neuro.reset_inference_state();
        }

        let eye = aim_target::player_eye_position(ctx)?;
        let (bmin, bmax) = get_aabb(ctx, &target)?;
        let height = bmax.y - bmin.y;
        let cur = aim_target::player_rotation(ctx)?;
        self.current_rotations = Rotation::new(cur.yaw as f32, cur.pitch as f32);

        let target_rot: Rotation = match cfg.rotation_type.as_str() {
            
            ROT_SMOOTH => {
                let point = Vec3d {
                    x: (bmin.x + bmax.x) * 0.5,
                    y: bmin.y + height * 0.6,
                    z: (bmin.z + bmax.z) * 0.5,
                };
                let (yaw, pitch) = RotationUtils::get_rotations(eye, point);
                Rotation::new(yaw, pitch)
            }
            
            
            ROT_SNAP => {
                let point = Multipoint::get_closest_point(eye, bmin, bmax);
                let predicted = if is_gliding(ctx, &target)? {
                    let vel = get_velocity(ctx, &target)?;
                    Predict::predict(point, vel, 2)
                } else {
                    point
                };
                let (yaw, pitch) = RotationUtils::get_rotations(eye, predicted);
                Rotation::new(yaw, pitch)
            }
            
            ROT_DATA => {
                let (yaw, pitch) = self.data_rotation(ctx, cfg, id, eye, bmin, bmax, height)?;
                Rotation::new(yaw, pitch)
            }
            
            ROT_SLOTH => {
                let (rp, rm, world) = BestPoint::get_multipoint(bmin, bmax, cfg.range as f64);
                self.rotation_point = rp;
                self.rotation_motion = rm;
                let (yaw, pitch) = RotationUtils::get_rotations(eye, world);
                Rotation::new(yaw, pitch)
            }
            
            ROT_NO_ROTATE => self.current_rotations,
            _ => {
                
                let point = Vec3d {
                    x: (bmin.x + bmax.x) * 0.5,
                    y: bmin.y + height * 0.6,
                    z: (bmin.z + bmax.z) * 0.5,
                };
                let predicted = if is_gliding(ctx, &target)? {
                    let vel = get_velocity(ctx, &target)?;
                    Predict::predict(point, vel, 2)
                } else {
                    point
                };
                let (yaw, pitch) = RotationUtils::get_rotations(eye, predicted);
                Rotation::new(yaw, pitch)
            }
        };

        let _ = ctx.env.delete_local_ref(target);
        self.target_rotations = target_rot;

        
        
        let gcd = rotation::mouse_step(ctx).unwrap_or(0.15);
        let yaw = rotation::snap_delta(target_rot.yaw as f64, gcd);
        let pitch = rotation::snap_delta(target_rot.pitch as f64, gcd);
        let cur = aim_target::player_rotation(ctx)?;
        rotation_storage::update(target_rot, cfg.client_look, cur.yaw as f32, cur.pitch as f32);
        if cfg.client_look {
            aim_target::set_player_rotation(ctx, rotation::Angle::new(yaw, pitch))
        } else {
            
            Ok(())
        }
    }

    
    #[allow(clippy::too_many_arguments)]
    fn data_rotation(
        &mut self,
        ctx: &mut MinecraftContext<'_, '_>,
        cfg: &Cfg,
        target_id: i32,
        eye: Vec3d,
        bmin: Vec3d,
        bmax: Vec3d,
        height: f64,
    ) -> JniResult<(f32, f32)> {
        
        
        let center = Vec3d {
            x: (bmin.x + bmax.x) * 0.5,
            y: (bmin.y + bmax.y) * 0.5,
            z: (bmin.z + bmax.z) * 0.5,
        };
        let stable = Vec3d {
            x: center.x,
            y: bmin.y + height * 0.72,
            z: center.z,
        };
        let fallback_point = if eye.distance_to(stable) <= 1.5 {
            stable
        } else {
            BestPoint::get_nearest_point(bmin, bmax, eye)
        };
        let fallback_rot = RotationUtils::get_rotations(eye, fallback_point);

        let cur = aim_target::player_rotation(ctx)?;
        let yaw_delta = rotation::wrap_degrees((fallback_rot.0 - cur.yaw as f32) as f64).abs();
        let pitch_delta = (fallback_rot.1 - cur.pitch as f32).abs();
        let hard_acquire = yaw_delta > 70.0 || (yaw_delta > 42.0 && eye.distance_to(center) < 3.0);

        
        let mut neuro_rot: Option<(f64, f64)> = None;
        if !hard_acquire {
            let now = time::now_ms();
            let dt = self
                .last_frame_at
                .take()
                .map(|last| (std::time::Instant::now().duration_since(last).as_secs_f64()).min(0.25))
                .unwrap_or(1.0 / 20.0)
                .clamp(1.0 / 240.0, 0.25);
            self.last_frame_at = Some(Instant::now());

            let target_pos = Vec3d { x: center.x, y: bmin.y, z: center.z };
            let (vel_x, vel_y, vel_z) = match self.prev_target_pos {
                Some(prev) if dt > 1e-4 => (
                    (target_pos.x - prev.x) / dt,
                    (target_pos.y - prev.y) / dt,
                    (target_pos.z - prev.z) / dt,
                ),
                _ => (0.0, 0.0, 0.0),
            };
            self.prev_target_pos = Some(target_pos);

            let yaw_error = rotation::wrap_degrees(fallback_rot.0 as f64 - cur.yaw);
            let pitch_error = fallback_rot.1 as f64 - cur.pitch;
            let human_d_yaw = rotation::wrap_degrees(cur.yaw - self.current_rotations.yaw as f64);
            let human_d_pitch = cur.pitch - self.current_rotations.pitch as f64;
            let d_yaw = if dt > 0.0 { (yaw_error - self.prev_yaw_error) / dt } else { 0.0 };
            let d_pitch = if dt > 0.0 { (pitch_error - self.prev_pitch_error) / dt } else { 0.0 };
            self.prev_yaw_error = yaw_error;
            self.prev_pitch_error = pitch_error;

            let result = self.neuro.tick(NeuroFrameInput {
                yaw_error,
                pitch_error,
                d_yaw_error: d_yaw,
                d_pitch_error: d_pitch,
                distance: eye.distance_to(center),
                rel_x: center.x - eye.x,
                rel_y: center.y - eye.y,
                rel_z: center.z - eye.z,
                target_vel_x: vel_x,
                target_vel_y: vel_y,
                target_vel_z: vel_z,
                view_yaw_degrees: cur.yaw,
                half_fov: 90.0,
                mouse_step: rotation::mouse_step(ctx).unwrap_or(0.15),
                dt,
                frames_since_acquire: 60.0,
                time_since_attack: (now.saturating_sub(self.last_attack_at)) as f64 / 1000.0,
                attack_held: false,
                on_ground: aim_target::player_on_ground(ctx).unwrap_or(true),
                sprint: minecraft::player_is_sprinting(ctx).unwrap_or(false),
                sneak: false,
                fall_distance: aim_target::player_fall_distance(ctx).unwrap_or(0.0),
                attack_strength: attack_strength_scale(ctx)? as f64,
                target_health: get_health(ctx, target_id)?,
                prev_yaw_delta: self.last_human_yaw_delta,
                prev_pitch_delta: self.last_human_pitch_delta,
                accel_yaw_delta: 0.0,
                accel_pitch_delta: 0.0,
                human_d_yaw,
                human_d_pitch,
                session_break: false,
                has_valid_target: true,
            });
            self.last_human_yaw_delta = human_d_yaw;
            self.last_human_pitch_delta = human_d_pitch;
            if result.authority == AimAuthority::NeuroAssist
                && let Some((dy, dp)) = result.neuro_delta
            {
                neuro_rot = Some((
                    cur.yaw + dy.clamp(-6.0, 6.0),
                    (cur.pitch + dp.clamp(-4.0, 4.0)).clamp(-90.0, 90.0),
                ));
            }
        }

        
        let (mut yaw, mut pitch) = match neuro_rot {
            Some((y, p)) => (y as f32, p as f32),
            None => fallback_rot,
        };

        
        let focus = self.should_focus_data_rotation(ctx);
        let (yaw_speed, pitch_speed) = if hard_acquire {
            (
                ((yaw_delta as f64) * 1.45).clamp(95.0, 180.0) as f32,
                ((pitch_delta as f64) * 1.35).clamp(55.0, 110.0) as f32,
            )
        } else if focus {
            (24.0_f32, 18.0_f32)
        } else {
            (11.5_f32, 9.0_f32)
        };
        yaw = step_toward(cur.yaw as f32, crate::utils::aura::math::wrap_degrees(yaw), yaw_speed);
        pitch = step_toward(cur.pitch as f32, pitch, pitch_speed);
        Ok((
            crate::utils::aura::math::wrap_degrees(yaw),
            pitch.clamp(-90.0, 90.0),
        ))
    }

    
    fn should_focus_data_rotation(&self, ctx: &mut MinecraftContext<'_, '_>) -> bool {
        let Ok(cooldown) = attack_strength_scale(ctx) else {
            return false;
        };
        let threshold = (IdealHit::get_ai_cooldown(ctx) - 0.08).max(0.82);
        let ready_by_cooldown = cooldown >= threshold;
        let falling = !aim_target::player_on_ground(ctx).unwrap_or(true)
            && aim_target::player_velocity_y(ctx).unwrap_or(0.0) < 0.0
            && aim_target::player_fall_distance(ctx).unwrap_or(0.0) > 0.0;
        ready_by_cooldown || falling
    }

    fn data_target_rotation(&self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg, target_id: i32) -> JniResult<(f32, f32)> {
        let Some(target) = ctx.entity_by_id(target_id)? else {
            return Ok((self.target_rotations.yaw, self.target_rotations.pitch));
        };
        let eye = aim_target::player_eye_position(ctx)?;
        let (bmin, bmax) = get_aabb(ctx, &target)?;
        let _ = ctx.env.delete_local_ref(target);
        let height = bmax.y - bmin.y;
        let (_, _, world) = BestPoint::get_multipoint(bmin, bmax, cfg.range as f64);
        let _ = height;
        Ok(RotationUtils::get_rotations(eye, world))
    }

    
    fn prepare_snap_attack(&mut self, ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg, id: i32) -> JniResult<bool> {
        if !self.snap_attack_queued {
            self.snap_attack_queued = true;
            self.snap_attack_age = get_age(ctx)? + 1;
            self.snap_attack_target = Some(id);
            return Ok(false);
        }
        let age = get_age(ctx)?;
        if age > self.snap_attack_age {
            self.reset_snap_attack();
            return Ok(false);
        }
        let _ = cfg;
        self.is_snap_aim_ready(ctx, id)
    }

    
    fn should_use_queued_snap_attack(&self, ctx: &mut MinecraftContext<'_, '_>, id: i32) -> JniResult<bool> {
        if !self.snap_attack_queued || self.target_id != Some(id) || self.snap_attack_target != Some(id) {
            return Ok(false);
        }
        let age = get_age(ctx)?;
        if age > self.snap_attack_age + 1 {
            return Ok(false);
        }
        Ok(age >= self.snap_attack_age)
    }

    
    fn is_snap_aim_ready(&self, ctx: &mut MinecraftContext<'_, '_>, target_id: i32) -> JniResult<bool> {
        let Some(target) = ctx.entity_by_id(target_id)? else {
            return Ok(false);
        };
        let eye = aim_target::player_eye_position(ctx)?;
        let (bmin, bmax) = get_aabb(ctx, &target)?;
        let _ = ctx.env.delete_local_ref(target);
        let center = Vec3d {
            x: (bmin.x + bmax.x) * 0.5,
            y: (bmin.y + bmax.y) * 0.5,
            z: (bmin.z + bmax.z) * 0.5,
        };
        let (tyaw, tpitch) = RotationUtils::get_rotations(eye, center);
        let cur = aim_target::player_rotation(ctx)?;
        let yaw_diff = crate::utils::aura::math::wrap_degrees(tyaw - cur.yaw as f32).abs();
        let pitch_diff = (tpitch - cur.pitch as f32).abs();
        let on_target = ray_trace_hit(eye, view_vector(ctx)?, 6.0, bmin, bmax);
        Ok(yaw_diff <= SNAP_YAW_TOLERANCE && pitch_diff <= SNAP_PITCH_TOLERANCE && on_target)
    }
}






type Cxt<'a> = MinecraftContext<'a, 'a>;

fn player_iterable<'local>(
    env: &mut jni::JNIEnv<'local>,
    level: &JObject<'local>,
) -> JniResult<JObject<'local>> {
    match env.call_method(level, "players", "()Ljava/util/List;", &[]) {
        Ok(v) => v.l(),
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            env.call_method(level, "entitiesForRendering", "()Ljava/lang/Iterable;", &[])?
                .l()
        }
    }
}

fn get_aabb(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<(Vec3d, Vec3d)> {
    let aabb = ctx
        .env
        .call_method(e, "getBoundingBox", "()Lnet/minecraft/world/phys/AABB;", &[])?
        .l()?;
    let min = Vec3d {
        x: ctx.env.get_field(&aabb, "minX", "D")?.d()?,
        y: ctx.env.get_field(&aabb, "minY", "D")?.d()?,
        z: ctx.env.get_field(&aabb, "minZ", "D")?.d()?,
    };
    let max = Vec3d {
        x: ctx.env.get_field(&aabb, "maxX", "D")?.d()?,
        y: ctx.env.get_field(&aabb, "maxY", "D")?.d()?,
        z: ctx.env.get_field(&aabb, "maxZ", "D")?.d()?,
    };
    let _ = ctx.env.delete_local_ref(aabb);
    Ok((min, max))
}

fn get_velocity(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<Vec3d> {
    let v = ctx
        .env
        .call_method(e, "getDeltaMovement", "()Lnet/minecraft/world/phys/Vec3;", &[])?
        .l()?;
    let out = Vec3d {
        x: ctx.env.get_field(&v, "x", "D")?.d()?,
        y: ctx.env.get_field(&v, "y", "D")?.d()?,
        z: ctx.env.get_field(&v, "z", "D")?.d()?,
    };
    let _ = ctx.env.delete_local_ref(v);
    Ok(out)
}

fn is_gliding(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<bool> {
    
    match ctx.env.call_method(e, "isFallFlying", "()Z", &[]) {
        Ok(v) => v.z(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env.call_method(e, "isGliding", "()Z", &[])?.z()
        }
    }
}

fn is_sprinting(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    ctx.env.call_method(&ctx.player, "isSprinting", "()Z", &[])?.z()
}

fn is_using_item(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    ctx.env.call_method(&ctx.player, "isUsingItem", "()Z", &[])?.z()
}

fn get_age(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<i32> {
    
    match ctx.env.get_field(&ctx.player, "tickCount", "I") {
        Ok(v) => v.i(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env.call_method(&ctx.player, "tickCount", "()I", &[])?.i()
        }
    }
}

fn get_health(ctx: &mut MinecraftContext<'_, '_>, id: i32) -> JniResult<f64> {
    let Some(e) = ctx.entity_by_id(id)? else {
        return Ok(20.0);
    };
    let h = ctx.env.call_method(&e, "getHealth", "()F", &[])?.f().unwrap_or(20.0);
    let _ = ctx.env.delete_local_ref(e);
    Ok(h as f64)
}


fn attack_strength_scale(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f32> {
    match ctx
        .env
        .call_method(&ctx.player, "getAttackStrengthScale", "(F)F", &[JValue::Float(1.5)])
    {
        Ok(v) => v.f(),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env
                .call_method(&ctx.player, "getAttackCooldownProgress", "(F)F", &[JValue::Float(1.5)])?
                .f()
        }
    }
}

fn view_vector(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    let v = ctx
        .env
        .call_method(&ctx.player, "getViewVector", "(F)Lnet/minecraft/world/phys/Vec3;", &[JValue::Float(1.0)])?
        .l()?;
    let out = Vec3d {
        x: ctx.env.get_field(&v, "x", "D")?.d()?,
        y: ctx.env.get_field(&v, "y", "D")?.d()?,
        z: ctx.env.get_field(&v, "z", "D")?.d()?,
    };
    let _ = ctx.env.delete_local_ref(v);
    Ok(out)
}

fn entity_can_see(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<bool> {
    ctx.env
        .call_method(&ctx.player, "hasLineOfSight", "(Lnet/minecraft/world/entity/Entity;)Z", &[JValue::Object(e)])?
        .z()
}


fn ray_trace_hit(camera: Vec3d, look: Vec3d, range: f64, bmin: Vec3d, bmax: Vec3d) -> bool {
    ray_trace::ray_trace_box(camera, look, range, bmin, bmax)
}

fn entity_ray_hit(ctx: &mut MinecraftContext<'_, '_>, id: i32, eye: Vec3d, look: Vec3d, reach: f64) -> JniResult<bool> {
    let Some(e) = ctx.entity_by_id(id)? else {
        return Ok(false);
    };
    let (bmin, bmax) = get_aabb(ctx, &e)?;
    let _ = ctx.env.delete_local_ref(e);
    Ok(ray_trace_hit(eye, look, reach, bmin, bmax))
}


fn max_aim_range(ctx: &mut MinecraftContext<'_, '_>, cfg: &Cfg) -> f64 {
    let gliding = ctx
        .env
        .call_method(&ctx.player, "isFallFlying", "()Z", &[])
        .and_then(|v| v.z())
        .unwrap_or(false);
    if gliding {
        cfg.elytra_aim_range as f64
    } else {
        (cfg.range + cfg.aim_range) as f64
    }
}


fn is_valid_target(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>, cfg: &Cfg) -> JniResult<bool> {
    if ctx.env.is_same_object(e, &ctx.player)? {
        return Ok(false);
    }
    let alive = ctx.env.call_method(e, "isAlive", "()Z", &[])?.z()?;
    if !alive {
        return Ok(false);
    }
    let health = ctx.env.call_method(e, "getHealth", "()F", &[])?.f().unwrap_or(0.0);
    if health <= 0.0 {
        return Ok(false);
    }

    let player_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let is_player = ctx.env.is_instance_of(e, &player_cls)?;
    if is_player {
        if !cfg.t_players {
            return Ok(false);
        }
        
        let name = entity_name(ctx, e)?;
        if friends::is_friend(&name) {
            return Ok(false);
        }
        
        let naked = is_naked(ctx, e)?;
        if naked && !cfg.t_naked {
            return Ok(false);
        }
        
        if !cfg.t_invis && is_invisible(ctx, e)? {
            return Ok(false);
        }
        return Ok(true);
    }
    
    
    Ok(cfg.t_mobs)
}

fn entity_name(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<String> {
    let name = ctx
        .env
        .call_method(e, "getName", "()Lnet/minecraft/network/chat/Component;", &[])?
        .l()?;
    let s = ctx
        .env
        .call_method(&name, "getString", "()Ljava/lang/String;", &[])?
        .l()?;
    let text = if s.is_null() {
        String::new()
    } else {
        let j = jni::objects::JString::from(s);
        let t = ctx.env.get_string(&j)?.to_string_lossy().into_owned();
        let _ = ctx.env.delete_local_ref(j);
        t
    };
    let _ = ctx.env.delete_local_ref(name);
    Ok(text)
}

fn is_naked(ctx: &mut MinecraftContext<'_, '_>, player: &JObject<'_>) -> JniResult<bool> {
    
    let slot_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.EquipmentSlot")?;
    for slot in ["HEAD", "CHEST", "LEGS", "FEET"] {
        let eq = ctx
            .env
            .get_static_field(&slot_cls, slot, "Lnet/minecraft/world/entity/EquipmentSlot;")?
            .l()?;
        let stack = ctx.env.call_method(
            player,
            "getItemBySlot",
            "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;",
            &[JValue::Object(&eq)],
        )?
        .l()?;
        let empty = if stack.is_null() {
            true
        } else {
            let e = ctx.env.call_method(&stack, "isEmpty", "()Z", &[])?.z().unwrap_or(true);
            let _ = ctx.env.delete_local_ref(stack);
            e
        };
        let _ = ctx.env.delete_local_ref(eq);
        if !empty {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_invisible(ctx: &mut MinecraftContext<'_, '_>, e: &JObject<'_>) -> JniResult<bool> {
    ctx.env.call_method(e, "isInvisible", "()Z", &[])?.z()
}


fn get_blocking_shield_hand<'local>(env: &mut jni::JNIEnv<'local>, player: &JObject<'local>) -> JniResult<Option<JObject<'local>>> {
    let blocking = env.call_method(player, "isBlocking", "()Z", &[])?.z().unwrap_or(false);
    if !blocking {
        return Ok(None);
    }
    let hand = match env.call_method(player, "getUsedItemHand", "()Lnet/minecraft/world/InteractionHand;", &[]) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    if hand.is_null() {
        return Ok(None);
    }
    Ok(Some(hand))
}


fn find_axe_slot(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Option<i32>> {
    let inv = ctx
        .env
        .call_method(&ctx.player, "getInventory", "()Lnet/minecraft/world/entity/player/Inventory;", &[])?
        .l()?;
    let axe_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.item.AxeItem")?;
    let mut found = None;
    for i in 0..9 {
        let stack = ctx
            .env
            .call_method(&inv, "getItem", "(I)Lnet/minecraft/world/item/ItemStack;", &[JValue::Int(i)])?
            .l()?;
        if stack.is_null() {
            continue;
        }
        let empty = ctx.env.call_method(&stack, "isEmpty", "()Z", &[])?.z().unwrap_or(true);
        if !empty {
            let item = ctx.env.call_method(&stack, "getItem", "()Lnet/minecraft/world/item/Item;", &[])?.l()?;
            let is_axe = ctx.env.is_instance_of(&item, &axe_cls)?;
            let _ = ctx.env.delete_local_ref(item);
            if is_axe {
                found = Some(i);
                let _ = ctx.env.delete_local_ref(stack);
                break;
            }
        }
        let _ = ctx.env.delete_local_ref(stack);
    }
    let _ = ctx.env.delete_local_ref(inv);
    Ok(found)
}


fn zero_move_input(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let input = match ctx.env.get_field(&ctx.player, "input", "Lnet/minecraft/client/player/ClientInput;") {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return Ok(());
        }
    };
    if input.is_null() {
        return Ok(());
    }
    let input_cls = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Input")?;
    let empty = ctx.env.get_static_field(&input_cls, "EMPTY", "Lnet/minecraft/world/entity/player/Input;")?.l()?;
    let _ = ctx.env.set_field(&input, "keyPresses", "Lnet/minecraft/world/entity/player/Input;", JValue::Object(&empty));
    let _ = ctx.env.delete_local_ref(empty);
    let _ = ctx.env.delete_local_ref(input);
    Ok(())
}


fn step_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = crate::utils::aura::math::wrap_degrees(target - current);
    if delta.abs() <= max_step {
        target
    } else {
        crate::utils::aura::math::wrap_degrees(current + delta.signum() * max_step)
    }
}











