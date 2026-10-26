use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JClass, JObject, JString, JValue},
    sys::{JNI_FALSE, JNI_TRUE},
};

use crate::{
    jni_bridge,
    utils::{
        friends,
        geometry::{Aabb, Vec3d},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetKind {
    All,
    Players,
    Mobs,
}

pub struct TargetSearchConfig {
    pub target_kind: TargetKind,
    pub attack_distance: f64,
    pub attack_invisible: bool,
    pub invisible_with_armor: bool,
    pub only_armored: bool,
    pub line_of_sight: bool,
}

pub struct MinecraftContext<'env, 'local> {
    pub env: &'env mut JNIEnv<'local>,
    pub minecraft: JObject<'local>,
    pub level: JObject<'local>,
    pub player: JObject<'local>,
    pub game_mode: JObject<'local>,
    pub options: JObject<'local>,
}

impl<'env, 'local> MinecraftContext<'env, 'local> {
    pub fn new(env: &'env mut JNIEnv<'local>) -> JniResult<Option<Self>> {
        let minecraft_class = jni_bridge::load_class(env, "net.minecraft.client.Minecraft")?;
        let minecraft = env
            .call_static_method(
                minecraft_class,
                "getInstance",
                "()Lnet/minecraft/client/Minecraft;",
                &[],
            )?
            .l()?;
        if minecraft.is_null() {
            return Ok(None);
        }

        let level = env
            .get_field(
                &minecraft,
                "level",
                "Lnet/minecraft/client/multiplayer/ClientLevel;",
            )?
            .l()?;
        let player = env
            .get_field(
                &minecraft,
                "player",
                "Lnet/minecraft/client/player/LocalPlayer;",
            )?
            .l()?;
        let game_mode = env
            .get_field(
                &minecraft,
                "gameMode",
                "Lnet/minecraft/client/multiplayer/MultiPlayerGameMode;",
            )?
            .l()?;
        let options = env
            .get_field(&minecraft, "options", "Lnet/minecraft/client/Options;")?
            .l()?;

        if level.is_null() || player.is_null() || game_mode.is_null() || options.is_null() {
            return Ok(None);
        }

        Ok(Some(Self {
            env,
            minecraft,
            level,
            player,
            game_mode,
            options,
        }))
    }

    pub fn entity_by_id(&mut self, entity_id: i32) -> JniResult<Option<JObject<'local>>> {
        let entity = self
            .env
            .call_method(
                &self.level,
                "getEntity",
                "(I)Lnet/minecraft/world/entity/Entity;",
                &[JValue::Int(entity_id)],
            )?
            .l()?;

        if entity.is_null() {
            Ok(None)
        } else {
            Ok(Some(entity))
        }
    }
}

pub fn find_target_under_crosshair(
    ctx: &mut MinecraftContext<'_, '_>,
    config: &TargetSearchConfig,
) -> JniResult<Option<i32>> {
    let classes = EntityClasses {
        living: jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.LivingEntity")?,
        player: jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?,
        mob: jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.Mob")?,
    };

    let eye = player_eye_position(ctx)?;
    let view = player_view_vector(ctx)?;
    let end = eye.add(view.scale(config.attack_distance));

    let iterable = ctx
        .env
        .call_method(
            &ctx.level,
            "entitiesForRendering",
            "()Ljava/lang/Iterable;",
            &[],
        )?
        .l()?;
    let iterator = ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let mut best: Option<TargetCandidate> = None;
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entity = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        let candidate = target_candidate(ctx, &entity, &classes, config, eye, end)?;
        let _ = ctx.env.delete_local_ref(entity);

        if let Some(candidate) = candidate {
            if best
                .as_ref()
                .is_none_or(|current| candidate.hit_distance < current.hit_distance)
            {
                best = Some(candidate);
            }
        }
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);

    Ok(best.map(|candidate| candidate.entity_id))
}

pub fn vanilla_crosshair_entity_id(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Option<i32>> {
    let hit_result = ctx
        .env
        .get_field(
            &ctx.minecraft,
            "hitResult",
            "Lnet/minecraft/world/phys/HitResult;",
        )?
        .l()?;
    if hit_result.is_null() {
        return Ok(None);
    }

    let entity_hit_result_class =
        jni_bridge::load_class(ctx.env, "net.minecraft.world.phys.EntityHitResult")?;
    if !ctx
        .env
        .is_instance_of(&hit_result, &entity_hit_result_class)?
    {
        let _ = ctx.env.delete_local_ref(hit_result);
        return Ok(None);
    }

    let entity = ctx
        .env
        .call_method(
            &hit_result,
            "getEntity",
            "()Lnet/minecraft/world/entity/Entity;",
            &[],
        )?
        .l()?;
    let result = if entity.is_null() {
        None
    } else {
        Some(ctx.env.call_method(&entity, "getId", "()I", &[])?.i()?)
    };

    let _ = ctx.env.delete_local_ref(entity);
    let _ = ctx.env.delete_local_ref(hit_result);
    Ok(result)
}

pub fn has_player_and_world(ctx: &MinecraftContext<'_, '_>) -> bool {
    !ctx.minecraft.is_null() && !ctx.level.is_null() && !ctx.player.is_null()
}

pub fn player_is_using_item(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    ctx.env
        .call_method(&ctx.player, "isUsingItem", "()Z", &[])?
        .z()
}

pub fn level_game_time(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<i64> {
    ctx.env
        .call_method(&ctx.level, "getGameTime", "()J", &[])?
        .j()
}

pub fn player_has_weapon(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    let stack = ctx
        .env
        .call_method(
            &ctx.player,
            "getMainHandItem",
            "()Lnet/minecraft/world/item/ItemStack;",
            &[],
        )?
        .l()?;
    if stack.is_null() {
        return Ok(false);
    }

    let empty = ctx.env.call_method(&stack, "isEmpty", "()Z", &[])?.z()?;
    if empty {
        let _ = ctx.env.delete_local_ref(stack);
        return Ok(false);
    }

    let item = ctx
        .env
        .call_method(&stack, "getItem", "()Lnet/minecraft/world/item/Item;", &[])?
        .l()?;
    let registry_weapon = match item_registry_name(ctx.env, &item) {
        Ok(name) => is_weapon_registry_name(&name),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            false
        }
    };
    let result = item_is_instance_of(ctx.env, &item, "net.minecraft.world.item.SwordItem")?
        || item_is_instance_of(ctx.env, &item, "net.minecraft.world.item.AxeItem")?
        || item_is_instance_of(ctx.env, &item, "net.minecraft.world.item.TridentItem")?
        || item_is_instance_of(ctx.env, &item, "net.minecraft.world.item.MaceItem")?
        || registry_weapon;

    let _ = ctx.env.delete_local_ref(item);
    let _ = ctx.env.delete_local_ref(stack);
    Ok(result)
}

fn item_is_instance_of(
    env: &mut JNIEnv<'_>,
    item: &JObject<'_>,
    class_name: &str,
) -> JniResult<bool> {
    let item_class = match jni_bridge::load_class(env, class_name) {
        Ok(item_class) => item_class,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(false);
        }
    };

    env.is_instance_of(item, &item_class)
}

fn item_registry_name(env: &mut JNIEnv<'_>, item: &JObject<'_>) -> JniResult<String> {
    let string = env
        .call_method(item, "toString", "()Ljava/lang/String;", &[])?
        .l()?;
    java_string(env, string)
}

fn java_string(env: &mut JNIEnv<'_>, string: JObject<'_>) -> JniResult<String> {
    if string.is_null() {
        return Ok(String::new());
    }

    let string = JString::from(string);
    let value = env.get_string(&string)?.to_string_lossy().into_owned();
    let _ = env.delete_local_ref(JObject::from(string));
    Ok(value)
}

fn is_weapon_registry_name(name: &str) -> bool {
    let name = name.rsplit(':').next().unwrap_or(name);
    name == "mace" || name == "trident" || name.ends_with("_sword") || name.ends_with("_axe")
}

pub fn entity_is_alive(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
) -> JniResult<bool> {
    ctx.env.call_method(entity, "isAlive", "()Z", &[])?.z()
}

pub fn attack_strength(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f32> {
    ctx.env
        .call_method(
            &ctx.player,
            "getAttackStrengthScale",
            "(F)F",
            &[JValue::Float(0.0)],
        )?
        .f()
}

pub fn attack_entity(ctx: &mut MinecraftContext<'_, '_>, target: &JObject<'_>) -> JniResult<()> {
    ctx.env.call_method(
        &ctx.game_mode,
        "attack",
        "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/Entity;)V",
        &[JValue::Object(&ctx.player), JValue::Object(target)],
    )?;
    swing_main_hand(ctx)
}

pub fn queue_vanilla_attack_click(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    if friends::no_friend_damage_active() && crosshair_target_is_friend(ctx)? {
        clear_queued_attack_clicks(ctx)?;
        return Ok(());
    }

    let key = attack_key(ctx)?;
    let click_count = ctx.env.get_field(&key, "clickCount", "I")?.i()?;
    ctx.env.set_field(
        &key,
        "clickCount",
        "I",
        JValue::Int(click_count.saturating_add(1)),
    )?;
    let _ = ctx.env.delete_local_ref(key);
    Ok(())
}

pub fn clear_queued_attack_clicks(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let key = attack_key(ctx)?;
    ctx.env.set_field(&key, "clickCount", "I", JValue::Int(0))?;
    let _ = ctx.env.delete_local_ref(key);
    Ok(())
}

pub fn crosshair_target_is_friend(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    let Some(target_id) = vanilla_crosshair_entity_id(ctx)? else {
        return Ok(false);
    };
    let Some(target) = ctx.entity_by_id(target_id)? else {
        return Ok(false);
    };

    let result = entity_is_friend(ctx, &target)?;
    let _ = ctx.env.delete_local_ref(target);
    Ok(result)
}

pub fn crosshair_player_name_within_distance(
    ctx: &mut MinecraftContext<'_, '_>,
    max_distance: f64,
) -> JniResult<Option<String>> {
    let Some(target_id) = vanilla_crosshair_entity_id(ctx)? else {
        return Ok(None);
    };
    let Some(target) = ctx.entity_by_id(target_id)? else {
        return Ok(None);
    };

    let player_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.entity.player.Player")?;
    let result = if ctx.env.is_instance_of(&target, &player_class)?
        && target_distance(ctx, &target)? <= max_distance
    {
        Some(entity_scoreboard_name(ctx, &target)?)
    } else {
        None
    };

    let _ = ctx.env.delete_local_ref(target);
    Ok(result)
}

pub fn entity_is_friend(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
) -> JniResult<bool> {
    Ok(friends::is_friend(&entity_scoreboard_name(ctx, entity)?))
}

pub fn entity_scoreboard_name(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
) -> JniResult<String> {
    let name = ctx
        .env
        .call_method(entity, "getScoreboardName", "()Ljava/lang/String;", &[])?
        .l()?;
    java_string(ctx.env, name)
}

pub fn swing_main_hand(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let hand_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.InteractionHand")?;
    let main_hand = ctx
        .env
        .get_static_field(
            hand_class,
            "MAIN_HAND",
            "Lnet/minecraft/world/InteractionHand;",
        )?
        .l()?;
    ctx.env.call_method(
        &ctx.player,
        "swing",
        "(Lnet/minecraft/world/InteractionHand;)V",
        &[JValue::Object(&main_hand)],
    )?;
    let _ = ctx.env.delete_local_ref(main_hand);
    Ok(())
}

pub fn use_main_hand_item(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    let hand_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.InteractionHand")?;
    let main_hand = ctx
        .env
        .get_static_field(
            hand_class,
            "MAIN_HAND",
            "Lnet/minecraft/world/InteractionHand;",
        )?
        .l()?;
    ctx.env.call_method(
        &ctx.game_mode,
        "useItem",
        "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;",
        &[JValue::Object(&ctx.player), JValue::Object(&main_hand)],
    )?;
    ctx.env.call_method(
        &ctx.player,
        "swing",
        "(Lnet/minecraft/world/InteractionHand;)V",
        &[JValue::Object(&main_hand)],
    )?;
    let _ = ctx.env.delete_local_ref(main_hand);
    Ok(())
}

pub fn selected_hotbar_slot(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<i32> {
    let inventory = player_inventory(ctx)?;
    let selected = ctx
        .env
        .call_method(&inventory, "getSelectedSlot", "()I", &[])?
        .i()?;
    let _ = ctx.env.delete_local_ref(inventory);
    Ok(selected)
}

pub fn set_selected_hotbar_slot(ctx: &mut MinecraftContext<'_, '_>, slot: i32) -> JniResult<()> {
    let inventory = player_inventory(ctx)?;
    ctx.env.call_method(
        &inventory,
        "setSelectedSlot",
        "(I)V",
        &[JValue::Int(slot.clamp(0, 8))],
    )?;
    let _ = ctx.env.delete_local_ref(inventory);
    Ok(())
}

pub fn find_ender_pearl_inventory_menu_slot(
    ctx: &mut MinecraftContext<'_, '_>,
) -> JniResult<Option<i32>> {
    let inventory = player_inventory(ctx)?;
    let items = ctx
        .env
        .call_method(
            &inventory,
            "getNonEquipmentItems",
            "()Lnet/minecraft/core/NonNullList;",
            &[],
        )?
        .l()?;

    let mut main_inventory_slot = None;
    for inventory_slot in 0..36 {
        let stack = ctx
            .env
            .call_method(
                &items,
                "get",
                "(I)Ljava/lang/Object;",
                &[JValue::Int(inventory_slot)],
            )?
            .l()?;
        let is_pearl = item_stack_is_ender_pearl(ctx, &stack)?;
        let _ = ctx.env.delete_local_ref(stack);

        if !is_pearl {
            continue;
        }
        if inventory_slot < 9 {
            let _ = ctx.env.delete_local_ref(items);
            let _ = ctx.env.delete_local_ref(inventory);
            return Ok(Some(36 + inventory_slot));
        }
        if main_inventory_slot.is_none() {
            main_inventory_slot = Some(inventory_slot);
        }
    }

    let _ = ctx.env.delete_local_ref(items);
    let _ = ctx.env.delete_local_ref(inventory);
    Ok(main_inventory_slot)
}

pub fn swap_inventory_menu_slot_to_hotbar(
    ctx: &mut MinecraftContext<'_, '_>,
    inventory_menu_slot: i32,
    hotbar_slot: i32,
) -> JniResult<()> {
    let swap = container_input_constant(ctx.env, "SWAP")?;
    ctx.env.call_method(
        &ctx.game_mode,
        "handleContainerInput",
        "(IIILnet/minecraft/world/inventory/ContainerInput;Lnet/minecraft/world/entity/player/Player;)V",
        &[
            JValue::Int(0),
            JValue::Int(inventory_menu_slot),
            JValue::Int(hotbar_slot.clamp(0, 8)),
            JValue::Object(&swap),
            JValue::Object(&ctx.player),
        ],
    )?;
    let _ = ctx.env.delete_local_ref(swap);
    Ok(())
}

pub fn reset_attack_strength(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<()> {
    ctx.env
        .call_method(&ctx.player, "resetAttackStrengthTicker", "()V", &[])?;
    Ok(())
}

fn player_inventory<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    ctx.env
        .call_method(
            &ctx.player,
            "getInventory",
            "()Lnet/minecraft/world/entity/player/Inventory;",
            &[],
        )?
        .l()
}

fn item_stack_is_ender_pearl(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
) -> JniResult<bool> {
    if stack.is_null() || ctx.env.call_method(stack, "isEmpty", "()Z", &[])?.z()? {
        return Ok(false);
    }

    let item = ctx
        .env
        .call_method(stack, "getItem", "()Lnet/minecraft/world/item/Item;", &[])?
        .l()?;
    let items_class = jni_bridge::load_class(ctx.env, "net.minecraft.world.item.Items")?;
    let pearl = ctx
        .env
        .get_static_field(
            items_class,
            "ENDER_PEARL",
            "Lnet/minecraft/world/item/Item;",
        )?
        .l()?;
    let is_pearl = ctx.env.is_same_object(&item, &pearl)?;
    let _ = ctx.env.delete_local_ref(pearl);
    let _ = ctx.env.delete_local_ref(item);
    Ok(is_pearl)
}

fn container_input_constant<'local>(
    env: &mut JNIEnv<'local>,
    name: &str,
) -> JniResult<JObject<'local>> {
    let input_class = jni_bridge::load_class(env, "net.minecraft.world.inventory.ContainerInput")?;
    env.get_static_field(
        input_class,
        name,
        "Lnet/minecraft/world/inventory/ContainerInput;",
    )?
    .l()
}

pub fn set_player_sprinting(ctx: &mut MinecraftContext<'_, '_>, sprinting: bool) -> JniResult<()> {
    ctx.env.call_method(
        &ctx.player,
        "setSprinting",
        "(Z)V",
        &[JValue::Bool(if sprinting { JNI_TRUE } else { JNI_FALSE })],
    )?;
    Ok(())
}

pub fn player_is_sprinting(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    ctx.env
        .call_method(&ctx.player, "isSprinting", "()Z", &[])?
        .z()
}

pub fn sprint_key_down(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    let key = sprint_key(ctx)?;
    let down = ctx.env.call_method(&key, "isDown", "()Z", &[])?.z()?;
    let _ = ctx.env.delete_local_ref(key);
    Ok(down)
}

pub fn set_sprint_key_down(ctx: &mut MinecraftContext<'_, '_>, down: bool) -> JniResult<()> {
    let key = sprint_key(ctx)?;
    ctx.env.call_method(
        &key,
        "setDown",
        "(Z)V",
        &[JValue::Bool(if down { JNI_TRUE } else { JNI_FALSE })],
    )?;
    let _ = ctx.env.delete_local_ref(key);
    Ok(())
}

pub fn target_distance(ctx: &mut MinecraftContext<'_, '_>, target: &JObject<'_>) -> JniResult<f64> {
    let eye = player_eye_position(ctx)?;
    Ok(entity_aabb(ctx, target)?.distance_to_point(eye))
}

pub fn player_distance_to_entity(
    ctx: &mut MinecraftContext<'_, '_>,
    target: &JObject<'_>,
) -> JniResult<f32> {
    ctx.env
        .call_method(
            &ctx.player,
            "distanceTo",
            "(Lnet/minecraft/world/entity/Entity;)F",
            &[JValue::Object(target)],
        )?
        .f()
}

pub fn player_in_blocking_block(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    let pos = ctx
        .env
        .call_method(
            &ctx.player,
            "blockPosition",
            "()Lnet/minecraft/core/BlockPos;",
            &[],
        )?
        .l()?;
    let up = ctx
        .env
        .call_method(&pos, "above", "()Lnet/minecraft/core/BlockPos;", &[])?
        .l()?;
    let eye = player_eye_position(ctx)?;
    let block_pos_class = jni_bridge::load_class(ctx.env, "net.minecraft.core.BlockPos")?;
    let eye_pos = ctx
        .env
        .call_static_method(
            block_pos_class,
            "containing",
            "(DDD)Lnet/minecraft/core/BlockPos;",
            &[
                JValue::Double(eye.x),
                JValue::Double(eye.y),
                JValue::Double(eye.z),
            ],
        )?
        .l()?;

    let result = block_is_blocking(ctx, &pos)?
        || block_is_blocking(ctx, &up)?
        || block_is_blocking(ctx, &eye_pos)?;

    let _ = ctx.env.delete_local_ref(eye_pos);
    let _ = ctx.env.delete_local_ref(up);
    let _ = ctx.env.delete_local_ref(pos);
    Ok(result)
}

pub fn player_in_special_movement(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    Ok(bool_method(ctx, "isInWater")?
        || bool_method(ctx, "isSwimming")?
        || bool_method(ctx, "isFallFlying")?
        || bool_method(ctx, "onClimbable")?
        || bool_method(ctx, "isInWall")?
        || bool_method(ctx, "isInLava")?
        || bool_method(ctx, "isPassenger")?)
}

pub fn player_can_critical(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    if bool_method(ctx, "onGround")?
        || bool_method(ctx, "isInWater")?
        || bool_method(ctx, "isInLava")?
        || bool_method(ctx, "isPassenger")?
    {
        return Ok(false);
    }

    Ok(player_velocity_y(ctx)? < -0.15)
}

pub fn player_on_ground(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<bool> {
    bool_method(ctx, "onGround")
}

pub fn player_velocity_y(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f64> {
    let velocity = ctx
        .env
        .call_method(
            &ctx.player,
            "getDeltaMovement",
            "()Lnet/minecraft/world/phys/Vec3;",
            &[],
        )?
        .l()?;
    let y = ctx.env.get_field(&velocity, "y", "D")?.d()?;
    let _ = ctx.env.delete_local_ref(velocity);
    Ok(y)
}

struct EntityClasses<'local> {
    living: JClass<'local>,
    player: JClass<'local>,
    mob: JClass<'local>,
}

struct TargetCandidate {
    entity_id: i32,
    hit_distance: f64,
}

fn target_candidate(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    classes: &EntityClasses<'_>,
    config: &TargetSearchConfig,
    eye: Vec3d,
    end: Vec3d,
) -> JniResult<Option<TargetCandidate>> {
    if ctx.env.is_same_object(entity, &ctx.player)?
        || !ctx.env.is_instance_of(entity, &classes.living)?
        || !target_kind_matches(ctx, entity, classes, config.target_kind)?
        || !ctx
            .env
            .call_method(entity, "canBeHitByProjectile", "()Z", &[])?
            .z()?
        || !passes_invisible_filter(ctx, entity, config)?
        || config.only_armored
            && ctx.env.is_instance_of(entity, &classes.player)?
            && !has_armor(ctx, entity)?
        || config.line_of_sight && !has_line_of_sight(ctx, entity)?
    {
        return Ok(None);
    }

    if friends::no_friend_damage_active()
        && ctx.env.is_instance_of(entity, &classes.player)?
        && entity_is_friend(ctx, entity)?
    {
        return Ok(None);
    }

    let aabb = entity_aabb(ctx, entity)?;
    if aabb.distance_to_point(eye) > config.attack_distance + 1.0 {
        return Ok(None);
    }

    let pick_box = aabb.inflate(entity_pick_radius(ctx, entity)? as f64);
    let hit = if pick_box.contains(eye) {
        Some(eye)
    } else {
        pick_box.ray_intersection(eye, end)
    };

    let Some(hit) = hit else {
        return Ok(None);
    };

    let box_distance = aabb.distance_to_point(eye);
    if box_distance > config.attack_distance {
        return Ok(None);
    }

    Ok(Some(TargetCandidate {
        entity_id: ctx.env.call_method(entity, "getId", "()I", &[])?.i()?,
        hit_distance: eye.distance_to(hit),
    }))
}

fn target_kind_matches(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    classes: &EntityClasses<'_>,
    target_kind: TargetKind,
) -> JniResult<bool> {
    match target_kind {
        TargetKind::All => Ok(true),
        TargetKind::Players => ctx.env.is_instance_of(entity, &classes.player),
        TargetKind::Mobs => ctx.env.is_instance_of(entity, &classes.mob),
    }
}

fn passes_invisible_filter(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    config: &TargetSearchConfig,
) -> JniResult<bool> {
    let invisible = ctx
        .env
        .call_method(entity, "isInvisible", "()Z", &[])?
        .z()?;
    if !invisible {
        return Ok(true);
    }

    Ok(config.attack_invisible && (!config.invisible_with_armor || has_armor(ctx, entity)?))
}

fn has_armor(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> JniResult<bool> {
    Ok(ctx
        .env
        .call_method(entity, "getArmorCoverPercentage", "()F", &[])?
        .f()?
        > 0.0)
}

fn has_line_of_sight(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> JniResult<bool> {
    ctx.env
        .call_method(
            &ctx.player,
            "hasLineOfSight",
            "(Lnet/minecraft/world/entity/Entity;)Z",
            &[JValue::Object(entity)],
        )?
        .z()
}

fn entity_pick_radius(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> JniResult<f32> {
    ctx.env
        .call_method(entity, "getPickRadius", "()F", &[])?
        .f()
}

fn player_eye_position(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    let vector = ctx
        .env
        .call_method(
            &ctx.player,
            "getEyePosition",
            "()Lnet/minecraft/world/phys/Vec3;",
            &[],
        )?
        .l()?;
    let result = vec3_from_java(ctx.env, &vector)?;
    let _ = ctx.env.delete_local_ref(vector);
    Ok(result)
}

fn player_view_vector(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Vec3d> {
    let vector = ctx
        .env
        .call_method(
            &ctx.player,
            "getViewVector",
            "(F)Lnet/minecraft/world/phys/Vec3;",
            &[JValue::Float(1.0)],
        )?
        .l()?;
    let result = vec3_from_java(ctx.env, &vector)?;
    let _ = ctx.env.delete_local_ref(vector);
    Ok(result)
}

pub fn entity_aabb(ctx: &mut MinecraftContext<'_, '_>, entity: &JObject<'_>) -> JniResult<Aabb> {
    let aabb = ctx
        .env
        .call_method(
            entity,
            "getBoundingBox",
            "()Lnet/minecraft/world/phys/AABB;",
            &[],
        )?
        .l()?;
    let result = aabb_from_java(ctx.env, &aabb)?;
    let _ = ctx.env.delete_local_ref(aabb);
    Ok(result)
}

fn vec3_from_java(env: &mut JNIEnv<'_>, vector: &JObject<'_>) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: env.get_field(vector, "x", "D")?.d()?,
        y: env.get_field(vector, "y", "D")?.d()?,
        z: env.get_field(vector, "z", "D")?.d()?,
    })
}

fn aabb_from_java(env: &mut JNIEnv<'_>, aabb: &JObject<'_>) -> JniResult<Aabb> {
    Ok(Aabb {
        min_x: env.get_field(aabb, "minX", "D")?.d()?,
        min_y: env.get_field(aabb, "minY", "D")?.d()?,
        min_z: env.get_field(aabb, "minZ", "D")?.d()?,
        max_x: env.get_field(aabb, "maxX", "D")?.d()?,
        max_y: env.get_field(aabb, "maxY", "D")?.d()?,
        max_z: env.get_field(aabb, "maxZ", "D")?.d()?,
    })
}

fn sprint_key<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    ctx.env
        .get_field(
            &ctx.options,
            "keySprint",
            "Lnet/minecraft/client/KeyMapping;",
        )?
        .l()
}

fn attack_key<'local>(ctx: &mut MinecraftContext<'_, 'local>) -> JniResult<JObject<'local>> {
    ctx.env
        .get_field(
            &ctx.options,
            "keyAttack",
            "Lnet/minecraft/client/KeyMapping;",
        )?
        .l()
}

fn block_is_blocking(ctx: &mut MinecraftContext<'_, '_>, pos: &JObject<'_>) -> JniResult<bool> {
    let state = ctx
        .env
        .call_method(
            &ctx.level,
            "getBlockState",
            "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
            &[JValue::Object(pos)],
        )?
        .l()?;

    let is_air = ctx.env.call_method(&state, "isAir", "()Z", &[])?.z()?;
    let result = if is_air {
        false
    } else {
        let full_cube = ctx
            .env
            .call_method(
                &state,
                "isCollisionShapeFullBlock",
                "(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z",
                &[JValue::Object(&ctx.level), JValue::Object(pos)],
            )?
            .z()?;
        let solid_render = ctx
            .env
            .call_method(&state, "isSolidRender", "()Z", &[])?
            .z()?;
        full_cube && solid_render
    };

    let _ = ctx.env.delete_local_ref(state);
    Ok(result)
}

fn bool_method(ctx: &mut MinecraftContext<'_, '_>, name: &str) -> JniResult<bool> {
    ctx.env.call_method(&ctx.player, name, "()Z", &[])?.z()
}
