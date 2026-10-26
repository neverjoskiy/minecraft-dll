use std::sync::Mutex;

use jni::{
    errors::Result as JniResult,
    objects::{JClass, JObject, JValue},
};

use crate::{
    interface::Module,
    jni_bridge,
    utils::{
        geometry::Vec3d,
        minecraft::{self, MinecraftContext},
        settings,
    },
};

pub const COLOR_SETTING: &str = "Color";
pub const HIT_COLOR_SETTING: &str = "Hit color";
pub const MAX_TICKS_SETTING: &str = "Max ticks";
pub const SHOW_HIT_SETTING: &str = "Show hit marker";

#[derive(Clone, Debug)]
pub struct TrajectoryEntry {
    
    pub points: Vec<Vec3d>,
    pub color: [f32; 4],
    
    pub hit_point: Option<Vec3d>,
    pub hit_color: [f32; 4],
    pub show_hit: bool,
}

struct SimulatedPath {
    points: Vec<Vec3d>,
    hit_point: Option<Vec3d>,
}

#[derive(Clone, Copy)]
struct ProjectileParams {
    speed: f64,
    gravity: f64,
    drag: f64,
}

static CACHED_ENTRIES: Mutex<Vec<TrajectoryEntry>> = Mutex::new(Vec::new());

pub fn cached_entries() -> Vec<TrajectoryEntry> {
    CACHED_ENTRIES.lock().map(|c| c.clone()).unwrap_or_default()
}



pub fn tick(config: &Module) {
    if !config.enabled {
        if let Ok(mut cache) = CACHED_ENTRIES.lock() {
            cache.clear();
        }
        return;
    }

    let max_ticks = settings::slider(config, MAX_TICKS_SETTING, 60.0) as i32;
    let color = settings::color_picker(config, COLOR_SETTING, [0.33, 0.85, 1.0, 1.0]);
    let hit_color = settings::color_picker(config, HIT_COLOR_SETTING, [1.0, 0.35, 0.35, 1.0]);
    let show_hit = settings::checkbox(config, SHOW_HIT_SETTING, true);

    let entries = jni_bridge::with_env(|env| {
        let Some(mut ctx) = MinecraftContext::new(env)? else {
            return Ok(Vec::new());
        };
        if !minecraft::has_player_and_world(&ctx) {
            return Ok(Vec::new());
        }
        let result = collect_trajectories(&mut ctx, max_ticks, color, hit_color, show_hit);
        if result.is_err() {
            jni_bridge::clear_pending_exception(ctx.env);
        }
        result
    })
    .unwrap_or_default();

    if let Ok(mut cache) = CACHED_ENTRIES.lock() {
        *cache = entries;
    }
}

fn collect_trajectories(
    ctx: &mut MinecraftContext<'_, '_>,
    max_ticks: i32,
    color: [f32; 4],
    hit_color: [f32; 4],
    show_hit: bool,
) -> JniResult<Vec<TrajectoryEntry>> {
    let mut entries = Vec::new();

    match predict_player_shots(ctx, color, hit_color, show_hit, max_ticks) {
        Ok(preds) => entries.extend(preds),
        Err(_) => jni_bridge::clear_pending_exception(ctx.env),
    }

    if collect_in_flight(ctx, color, hit_color, show_hit, max_ticks, &mut entries).is_err() {
        jni_bridge::clear_pending_exception(ctx.env);
    }

    Ok(entries)
}

fn predict_player_shots(
    ctx: &mut MinecraftContext<'_, '_>,
    color: [f32; 4],
    hit_color: [f32; 4],
    show_hit: bool,
    max_ticks: i32,
) -> JniResult<Vec<TrajectoryEntry>> {
    let Some(held) = held_projectile(ctx)? else {
        return Ok(Vec::new());
    };

    let eye_y = ctx
        .env
        .call_method(&ctx.player, "getEyeY", "()D", &[])?
        .d()?;
    let pos_x = ctx.env.call_method(&ctx.player, "getX", "()D", &[])?.d()?;
    let pos_z = ctx.env.call_method(&ctx.player, "getZ", "()D", &[])?.d()?;
    let start = Vec3d {
        x: pos_x,
        y: eye_y - 0.1,
        z: pos_z,
    };

    let mut entries = Vec::with_capacity(held.yaw_offsets.len());
    for yaw_offset in held.yaw_offsets {
        let Some(dir_norm) = view_direction_with_yaw_offset(ctx, yaw_offset)?.normalize() else {
            continue;
        };
        let sim_vel = dir_norm.scale(held.params.speed);
        if let Some(path) = simulate_path(
            start,
            sim_vel,
            held.params.gravity,
            held.params.drag,
            max_ticks,
            ctx,
        ) {
            entries.push(TrajectoryEntry {
                points: path.points,
                color,
                hit_point: path.hit_point,
                hit_color,
                show_hit,
            });
        }
    }

    Ok(entries)
}

#[derive(Clone)]
struct HeldProjectile {
    params: ProjectileParams,
    
    yaw_offsets: Vec<f32>,
}

fn held_projectile(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<Option<HeldProjectile>> {
    let main = ctx
        .env
        .call_method(
            &ctx.player,
            "getMainHandItem",
            "()Lnet/minecraft/world/item/ItemStack;",
            &[],
        )?
        .l()?;
    let off = ctx
        .env
        .call_method(
            &ctx.player,
            "getOffhandItem",
            "()Lnet/minecraft/world/item/ItemStack;",
            &[],
        )?
        .l()?;

    let held = stack_projectile(ctx, &main)?.or(stack_projectile(ctx, &off)?);

    let _ = ctx.env.delete_local_ref(off);
    let _ = ctx.env.delete_local_ref(main);
    Ok(held)
}

fn stack_projectile(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
) -> JniResult<Option<HeldProjectile>> {
    if stack.is_null() || ctx.env.call_method(stack, "isEmpty", "()Z", &[])?.z()? {
        return Ok(None);
    }

    let item = ctx
        .env
        .call_method(stack, "getItem", "()Lnet/minecraft/world/item/Item;", &[])?
        .l()?;
    let kind = match item_projectile_kind(ctx, &item) {
        Ok(kind) => kind,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(item);
            return Ok(None);
        }
    };
    let _ = ctx.env.delete_local_ref(item);

    let Some(kind) = kind else {
        return Ok(None);
    };

    Ok(match kind {
        ProjectileKind::EnderPearl | ProjectileKind::Snowball | ProjectileKind::Egg => {
            Some(HeldProjectile {
                params: ProjectileParams {
                    speed: 1.5,
                    gravity: 0.03,
                    drag: 0.99,
                },
                yaw_offsets: vec![0.0],
            })
        }
        ProjectileKind::SplashPotion | ProjectileKind::LingeringPotion => Some(HeldProjectile {
            params: ProjectileParams {
                speed: 0.5,
                gravity: 0.05,
                drag: 0.99,
            },
            yaw_offsets: vec![0.0],
        }),
        ProjectileKind::Bow => {
            if !minecraft::player_is_using_item(ctx)? {
                return Ok(None);
            }
            let charge = ctx
                .env
                .call_method(&ctx.player, "getTicksUsingItem", "()I", &[])?
                .i()? as f64;
            let pull = (charge / 20.0).min(1.0);
            let power = (pull * pull + pull * 2.0) / 3.0;
            let speed = power * 3.0;
            if speed < 0.1 {
                None
            } else {
                let multishot = stack_multishot_level(ctx, stack).unwrap_or(0);
                Some(HeldProjectile {
                    params: ProjectileParams {
                        speed,
                        gravity: 0.05,
                        drag: 0.99,
                    },
                    yaw_offsets: multishot_yaw_offsets(multishot_projectile_count(multishot, 1)),
                })
            }
        }
        ProjectileKind::Crossbow => {
            if !crossbow_is_charged(ctx, stack)? {
                return Ok(None);
            }
            let charged = crossbow_charged_count(ctx, stack).unwrap_or(1).max(1);
            let multishot = stack_multishot_level(ctx, stack).unwrap_or(0);
            let count = charged.max(multishot_projectile_count(multishot, 1));
            Some(HeldProjectile {
                params: ProjectileParams {
                    speed: 3.15,
                    gravity: 0.05,
                    drag: 0.99,
                },
                yaw_offsets: multishot_yaw_offsets(count),
            })
        }
        ProjectileKind::Trident => {
            if !minecraft::player_is_using_item(ctx)? {
                return Ok(None);
            }
            let charge = ctx
                .env
                .call_method(&ctx.player, "getTicksUsingItem", "()I", &[])?
                .i()?;
            if charge < 10 {
                None
            } else {
                Some(HeldProjectile {
                    params: ProjectileParams {
                        speed: 2.5,
                        gravity: 0.05,
                        drag: 0.99,
                    },
                    yaw_offsets: vec![0.0],
                })
            }
        }
        ProjectileKind::WindCharge => Some(HeldProjectile {
            params: ProjectileParams {
                speed: 1.5,
                gravity: 0.0,
                drag: 1.0,
            },
            yaw_offsets: vec![0.0],
        }),
    })
}

fn multishot_projectile_count(multishot_level: i32, fallback: i32) -> i32 {
    if multishot_level <= 0 {
        fallback
    } else {
        
        multishot_level * 2 + 1
    }
}

fn multishot_yaw_offsets(projectile_count: i32) -> Vec<f32> {
    let n = projectile_count.max(1);
    if n == 1 {
        return vec![0.0];
    }
    let half = (n - 1) as f32 * 0.5;
    (0..n).map(|i| (i as f32 - half) * 10.0).collect()
}

fn view_direction_with_yaw_offset(
    ctx: &mut MinecraftContext<'_, '_>,
    yaw_offset_degrees: f32,
) -> JniResult<Vec3d> {
    if yaw_offset_degrees.abs() < f32::EPSILON {
        let view = ctx
            .env
            .call_method(
                &ctx.player,
                "getViewVector",
                "(F)Lnet/minecraft/world/phys/Vec3;",
                &[JValue::Float(1.0)],
            )?
            .l()?;
        let dir = vec3_from_java(ctx.env, &view)?;
        let _ = ctx.env.delete_local_ref(view);
        return Ok(dir);
    }

    let y_rot = ctx
        .env
        .call_method(&ctx.player, "getYRot", "()F", &[])?
        .f()?
        + yaw_offset_degrees;
    let x_rot = ctx
        .env
        .call_method(&ctx.player, "getXRot", "()F", &[])?
        .f()?;
    
    let pitch = (x_rot as f64).to_radians();
    let yaw = (-(y_rot as f64)).to_radians();
    let cos_pitch = pitch.cos();
    Ok(Vec3d {
        x: yaw.sin() * cos_pitch,
        y: -pitch.sin(),
        z: yaw.cos() * cos_pitch,
    })
}

#[derive(Clone, Copy)]
enum ProjectileKind {
    EnderPearl,
    Snowball,
    Egg,
    SplashPotion,
    LingeringPotion,
    Bow,
    Crossbow,
    Trident,
    WindCharge,
}

fn item_projectile_kind(
    ctx: &mut MinecraftContext<'_, '_>,
    item: &JObject<'_>,
) -> JniResult<Option<ProjectileKind>> {
    let items = jni_bridge::load_class(ctx.env, "net.minecraft.world.item.Items")?;
    let checks = [
        ("ENDER_PEARL", ProjectileKind::EnderPearl),
        ("SNOWBALL", ProjectileKind::Snowball),
        ("EGG", ProjectileKind::Egg),
        ("BLUE_EGG", ProjectileKind::Egg),
        ("BROWN_EGG", ProjectileKind::Egg),
        ("SPLASH_POTION", ProjectileKind::SplashPotion),
        ("LINGERING_POTION", ProjectileKind::LingeringPotion),
        ("BOW", ProjectileKind::Bow),
        ("CROSSBOW", ProjectileKind::Crossbow),
        ("TRIDENT", ProjectileKind::Trident),
        ("WIND_CHARGE", ProjectileKind::WindCharge),
    ];

    for (field, kind) in checks {
        let candidate =
            match ctx
                .env
                .get_static_field(&items, field, "Lnet/minecraft/world/item/Item;")
            {
                Ok(v) => v.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(ctx.env);
                    continue;
                }
            };
        let same = ctx.env.is_same_object(item, &candidate)?;
        let _ = ctx.env.delete_local_ref(candidate);
        if same {
            return Ok(Some(kind));
        }
    }

    Ok(None)
}

fn crossbow_is_charged(ctx: &mut MinecraftContext<'_, '_>, stack: &JObject<'_>) -> JniResult<bool> {
    let class = match jni_bridge::load_class(ctx.env, "net.minecraft.world.item.CrossbowItem") {
        Ok(c) => c,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return Ok(true);
        }
    };
    match ctx.env.call_static_method(
        class,
        "isCharged",
        "(Lnet/minecraft/world/item/ItemStack;)Z",
        &[JValue::Object(stack)],
    ) {
        Ok(v) => Ok(v.z()?),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            Ok(true)
        }
    }
}


fn crossbow_charged_count(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
) -> JniResult<i32> {
    let components =
        match jni_bridge::load_class(ctx.env, "net.minecraft.core.component.DataComponents") {
            Ok(c) => c,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                return Ok(1);
            }
        };
    let component_type = match ctx.env.get_static_field(
        &components,
        "CHARGED_PROJECTILES",
        "Lnet/minecraft/core/component/DataComponentType;",
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(components);
            return Ok(1);
        }
    };
    let charged = match ctx.env.call_method(
        stack,
        "get",
        "(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;",
        &[JValue::Object(&component_type)],
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(component_type);
            let _ = ctx.env.delete_local_ref(components);
            return Ok(1);
        }
    };
    let _ = ctx.env.delete_local_ref(component_type);
    let _ = ctx.env.delete_local_ref(components);
    if charged.is_null() {
        return Ok(1);
    }

    let list = match ctx
        .env
        .call_method(&charged, "itemCopies", "()Ljava/util/List;", &[])
    {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            match ctx
                .env
                .call_method(&charged, "items", "()Ljava/util/List;", &[])
            {
                Ok(v) => v.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(ctx.env);
                    let _ = ctx.env.delete_local_ref(charged);
                    return Ok(1);
                }
            }
        }
    };
    let size = ctx.env.call_method(&list, "size", "()I", &[])?.i()?;
    let _ = ctx.env.delete_local_ref(list);
    let _ = ctx.env.delete_local_ref(charged);
    Ok(size.max(1))
}

fn stack_multishot_level(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
) -> JniResult<i32> {
    if let Ok(level) = enchantment_level_via_helper(ctx, stack, "multishot") {
        if level > 0 {
            return Ok(level);
        }
    } else {
        jni_bridge::clear_pending_exception(ctx.env);
    }
    if let Ok(level) = enchantment_level_via_item_enchantments(ctx, stack, "multishot") {
        return Ok(level);
    }
    jni_bridge::clear_pending_exception(ctx.env);
    Ok(0)
}

fn enchantment_level_via_helper(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
    enchant_path: &str,
) -> JniResult<i32> {
    let holder = enchantment_holder(ctx, enchant_path)?;
    let Some(holder) = holder else {
        return Ok(0);
    };
    let helper = jni_bridge::load_class(
        ctx.env,
        "net.minecraft.world.item.enchantment.EnchantmentHelper",
    )?;
    let level = match ctx.env.call_static_method(
        &helper,
        "getItemEnchantmentLevel",
        "(Lnet/minecraft/core/Holder;Lnet/minecraft/world/item/ItemStack;)I",
        &[JValue::Object(&holder), JValue::Object(stack)],
    ) {
        Ok(v) => v.i()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env
                .call_static_method(
                    &helper,
                    "getLevel",
                    "(Lnet/minecraft/core/Holder;Lnet/minecraft/world/item/ItemStack;)I",
                    &[JValue::Object(&holder), JValue::Object(stack)],
                )?
                .i()?
        }
    };
    let _ = ctx.env.delete_local_ref(holder);
    Ok(level.max(0))
}

fn enchantment_holder<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    enchant_path: &str,
) -> JniResult<Option<JObject<'local>>> {
    let registry_access = ctx
        .env
        .call_method(
            &ctx.level,
            "registryAccess",
            "()Lnet/minecraft/core/RegistryAccess;",
            &[],
        )?
        .l()?;
    let registries = jni_bridge::load_class(ctx.env, "net.minecraft.core.registries.Registries")?;
    let enchantment_key = ctx
        .env
        .get_static_field(
            &registries,
            "ENCHANTMENT",
            "Lnet/minecraft/resources/ResourceKey;",
        )?
        .l()?;
    let lookup = match ctx.env.call_method(
        &registry_access,
        "lookupOrThrow",
        "(Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/HolderLookup$RegistryLookup;",
        &[JValue::Object(&enchantment_key)],
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env
                .call_method(
                    &registry_access,
                    "lookupOrThrow",
                    "(Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/HolderGetter;",
                    &[JValue::Object(&enchantment_key)],
                )?
                .l()?
        }
    };

    let multishot_key =
        enchantments_multishot_key(ctx)?.or(resource_key_enchantment(ctx, enchant_path)?);
    let Some(multishot_key) = multishot_key else {
        let _ = ctx.env.delete_local_ref(lookup);
        let _ = ctx.env.delete_local_ref(enchantment_key);
        let _ = ctx.env.delete_local_ref(registries);
        let _ = ctx.env.delete_local_ref(registry_access);
        return Ok(None);
    };

    let holder = match ctx.env.call_method(
        &lookup,
        "getOrThrow",
        "(Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/Holder$Reference;",
        &[JValue::Object(&multishot_key)],
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            ctx.env
                .call_method(
                    &lookup,
                    "getOrThrow",
                    "(Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/Holder;",
                    &[JValue::Object(&multishot_key)],
                )?
                .l()?
        }
    };

    let _ = ctx.env.delete_local_ref(multishot_key);
    let _ = ctx.env.delete_local_ref(lookup);
    let _ = ctx.env.delete_local_ref(enchantment_key);
    let _ = ctx.env.delete_local_ref(registries);
    let _ = ctx.env.delete_local_ref(registry_access);
    Ok(Some(holder))
}

fn enchantments_multishot_key<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
) -> JniResult<Option<JObject<'local>>> {
    let class = match jni_bridge::load_class(
        ctx.env,
        "net.minecraft.world.item.enchantment.Enchantments",
    ) {
        Ok(c) => c,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return Ok(None);
        }
    };
    match ctx
        .env
        .get_static_field(&class, "MULTISHOT", "Lnet/minecraft/resources/ResourceKey;")
    {
        Ok(v) => Ok(Some(v.l()?)),
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            Ok(None)
        }
    }
}

fn resource_key_enchantment<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    path: &str,
) -> JniResult<Option<JObject<'local>>> {
    let registries = jni_bridge::load_class(ctx.env, "net.minecraft.core.registries.Registries")?;
    let enchantment_reg = ctx
        .env
        .get_static_field(
            &registries,
            "ENCHANTMENT",
            "Lnet/minecraft/resources/ResourceKey;",
        )?
        .l()?;
    let id_string = ctx
        .env
        .new_string(format!("minecraft:{path}"))
        .map(JObject::from)?;
    let identifier = match new_identifier(ctx, &id_string) {
        Ok(id) => id,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(id_string);
            let _ = ctx.env.delete_local_ref(enchantment_reg);
            let _ = ctx.env.delete_local_ref(registries);
            return Ok(None);
        }
    };
    let resource_key_class =
        jni_bridge::load_class(ctx.env, "net.minecraft.resources.ResourceKey")?;
    let key = ctx
        .env
        .call_static_method(
            &resource_key_class,
            "create",
            "(Lnet/minecraft/resources/ResourceKey;Lnet/minecraft/resources/Identifier;)Lnet/minecraft/resources/ResourceKey;",
            &[JValue::Object(&enchantment_reg), JValue::Object(&identifier)],
        )?
        .l()?;
    let _ = ctx.env.delete_local_ref(identifier);
    let _ = ctx.env.delete_local_ref(id_string);
    let _ = ctx.env.delete_local_ref(enchantment_reg);
    let _ = ctx.env.delete_local_ref(registries);
    Ok(Some(key))
}

fn new_identifier<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    value: &JObject<'local>,
) -> JniResult<JObject<'local>> {
    ctx.env
        .call_static_method(
            "net/minecraft/resources/Identifier",
            "parse",
            "(Ljava/lang/String;)Lnet/minecraft/resources/Identifier;",
            &[JValue::Object(value)],
        )?
        .l()
}

fn enchantment_level_via_item_enchantments(
    ctx: &mut MinecraftContext<'_, '_>,
    stack: &JObject<'_>,
    enchant_path: &str,
) -> JniResult<i32> {
    let enchants = match ctx.env.call_method(
        stack,
        "getEnchantments",
        "()Lnet/minecraft/world/item/enchantment/ItemEnchantments;",
        &[],
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return Ok(0);
        }
    };
    if enchants.is_null() {
        return Ok(0);
    }

    let entry_set = ctx
        .env
        .call_method(&enchants, "entrySet", "()Ljava/util/Set;", &[])?
        .l()?;
    let iterator = ctx
        .env
        .call_method(&entry_set, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;

    let mut found = 0i32;
    while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
        let entry = ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        let holder = ctx
            .env
            .call_method(&entry, "getKey", "()Ljava/lang/Object;", &[])?
            .l()?;
        let level = ctx
            .env
            .call_method(&entry, "getIntValue", "()I", &[])?
            .i()?;
        let path = holder_enchantment_path(ctx, &holder).unwrap_or_default();
        if path == enchant_path {
            found = level.max(0);
            let _ = ctx.env.delete_local_ref(holder);
            let _ = ctx.env.delete_local_ref(entry);
            break;
        }
        let _ = ctx.env.delete_local_ref(holder);
        let _ = ctx.env.delete_local_ref(entry);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(entry_set);
    let _ = ctx.env.delete_local_ref(enchants);
    Ok(found)
}

fn holder_enchantment_path(
    ctx: &mut MinecraftContext<'_, '_>,
    holder: &JObject<'_>,
) -> JniResult<String> {
    let optional = ctx
        .env
        .call_method(holder, "unwrapKey", "()Ljava/util/Optional;", &[])?
        .l()?;
    if optional.is_null()
        || !ctx
            .env
            .call_method(&optional, "isPresent", "()Z", &[])?
            .z()?
    {
        let _ = ctx.env.delete_local_ref(optional);
        return Ok(String::new());
    }
    let resource_key = ctx
        .env
        .call_method(&optional, "get", "()Ljava/lang/Object;", &[])?
        .l()?;
    let location = ctx
        .env
        .call_method(
            &resource_key,
            "identifier",
            "()Lnet/minecraft/resources/Identifier;",
            &[],
        )?
        .l()?;
    let path = match ctx
        .env
        .call_method(&location, "getPath", "()Ljava/lang/String;", &[])
    {
        Ok(v) => {
            let string = v.l()?;
            let text = if string.is_null() {
                String::new()
            } else {
                let value = ctx
                    .env
                    .get_string((&string).into())?
                    .to_string_lossy()
                    .into_owned();
                let _ = ctx.env.delete_local_ref(string);
                value
            };
            text
        }
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            String::new()
        }
    };
    let _ = ctx.env.delete_local_ref(location);
    let _ = ctx.env.delete_local_ref(resource_key);
    let _ = ctx.env.delete_local_ref(optional);
    Ok(path)
}

fn collect_in_flight(
    ctx: &mut MinecraftContext<'_, '_>,
    color: [f32; 4],
    hit_color: [f32; 4],
    show_hit: bool,
    max_ticks: i32,
    entries: &mut Vec<TrajectoryEntry>,
) -> JniResult<()> {
    let iterable = match ctx.env.call_method(
        &ctx.level,
        "entitiesForRendering",
        "()Ljava/lang/Iterable;",
        &[],
    ) {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return Ok(());
        }
    };
    let iterator = match ctx
        .env
        .call_method(&iterable, "iterator", "()Ljava/util/Iterator;", &[])
    {
        Ok(v) => v.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(iterable);
            return Ok(());
        }
    };

    let throwable = jni_bridge::load_class(
        ctx.env,
        "net.minecraft.world.entity.projectile.ThrowableProjectile",
    )
    .ok();
    let arrow = jni_bridge::load_class(
        ctx.env,
        "net.minecraft.world.entity.projectile.arrow.AbstractArrow",
    )
    .ok();
    if throwable.is_none() && arrow.is_none() {
        jni_bridge::clear_pending_exception(ctx.env);
    }

    loop {
        let has_next = match ctx.env.call_method(&iterator, "hasNext", "()Z", &[]) {
            Ok(v) => v.z()?,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                break;
            }
        };
        if !has_next {
            break;
        }

        let entity = match ctx
            .env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])
        {
            Ok(v) => v.l()?,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                break;
            }
        };

        let is_throwable = throwable
            .as_ref()
            .map(|c| ctx.env.is_instance_of(&entity, c).unwrap_or(false))
            .unwrap_or(false);
        let is_arrow = arrow
            .as_ref()
            .map(|c| ctx.env.is_instance_of(&entity, c).unwrap_or(false))
            .unwrap_or(false);

        if !is_throwable && !is_arrow {
            let _ = ctx.env.delete_local_ref(entity);
            continue;
        }

        let (gravity, drag) = if is_throwable {
            (0.03, 0.99)
        } else {
            (0.05, 0.99)
        };

        match build_in_flight(
            ctx, &entity, color, hit_color, show_hit, max_ticks, gravity, drag,
        ) {
            Ok(Some(entry)) => entries.push(entry),
            Ok(None) => {}
            Err(_) => jni_bridge::clear_pending_exception(ctx.env),
        }

        let _ = ctx.env.delete_local_ref(entity);
    }

    let _ = ctx.env.delete_local_ref(iterator);
    let _ = ctx.env.delete_local_ref(iterable);
    Ok(())
}

fn build_in_flight(
    ctx: &mut MinecraftContext<'_, '_>,
    entity: &JObject<'_>,
    color: [f32; 4],
    hit_color: [f32; 4],
    show_hit: bool,
    max_ticks: i32,
    gravity: f64,
    drag: f64,
) -> JniResult<Option<TrajectoryEntry>> {
    let pos_x = ctx.env.call_method(entity, "getX", "()D", &[])?.d()?;
    let pos_y = ctx.env.call_method(entity, "getY", "()D", &[])?.d()?;
    let pos_z = ctx.env.call_method(entity, "getZ", "()D", &[])?.d()?;

    let motion = ctx
        .env
        .call_method(
            entity,
            "getDeltaMovement",
            "()Lnet/minecraft/world/phys/Vec3;",
            &[],
        )?
        .l()?;
    let vel = vec3_from_java(ctx.env, &motion)?;
    let _ = ctx.env.delete_local_ref(motion);

    if vel.x * vel.x + vel.y * vel.y + vel.z * vel.z < 0.0001 {
        return Ok(None);
    }

    let sim_pos = Vec3d {
        x: pos_x,
        y: pos_y,
        z: pos_z,
    };

    Ok(simulate_path(sim_pos, vel, gravity, drag, max_ticks, ctx)
    .map(|path| TrajectoryEntry {
        points: path.points,
        color,
        hit_point: path.hit_point,
        hit_color,
        show_hit,
    }))
}




fn simulate_path(
    mut pos: Vec3d,
    mut vel: Vec3d,
    gravity: f64,
    drag: f64,
    max_ticks: i32,
    ctx: &mut MinecraftContext<'_, '_>,
) -> Option<SimulatedPath> {
    let ticks = max_ticks.clamp(1, 300);
    let mut points: Vec<Vec3d> = Vec::with_capacity(ticks as usize + 1);
    let mut hit_point = None;
    let block_pos_class = jni_bridge::load_class(ctx.env, "net.minecraft.core.BlockPos").ok();

    points.push(pos);

    for _ in 0..ticks {
        
        vel.y -= gravity;
        vel.x *= drag;
        vel.y *= drag;
        vel.z *= drag;

        let next = Vec3d {
            x: pos.x + vel.x,
            y: pos.y + vel.y,
            z: pos.z + vel.z,
        };

        if next.y < -64.0 {
            break;
        }

        if vel.x * vel.x + vel.y * vel.y + vel.z * vel.z < 1.0e-8 {
            break;
        }

        if let Some(ref bp_class) = block_pos_class
            && block_blocks_projectile(ctx, bp_class, next)
        {
            points.push(next);
            hit_point = Some(next);
            break;
        }

        pos = next;
        points.push(pos);
    }

    if points.len() < 2 {
        None
    } else {
        Some(SimulatedPath { points, hit_point })
    }
}

fn block_blocks_projectile(
    ctx: &mut MinecraftContext<'_, '_>,
    block_pos_class: &JClass<'_>,
    pos: Vec3d,
) -> bool {
    let block_pos = match ctx.env.call_static_method(
        block_pos_class,
        "containing",
        "(DDD)Lnet/minecraft/core/BlockPos;",
        &[
            JValue::Double(pos.x),
            JValue::Double(pos.y),
            JValue::Double(pos.z),
        ],
    ) {
        Ok(v) => match v.l() {
            Ok(obj) => obj,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                return false;
            }
        },
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            return false;
        }
    };

    let state = match ctx.env.call_method(
        &ctx.level,
        "getBlockState",
        "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
        &[JValue::Object(&block_pos)],
    ) {
        Ok(v) => match v.l() {
            Ok(obj) => obj,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                let _ = ctx.env.delete_local_ref(block_pos);
                return false;
            }
        },
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(block_pos);
            return false;
        }
    };

    let is_air = ctx
        .env
        .call_method(&state, "isAir", "()Z", &[])
        .and_then(|v| v.z())
        .unwrap_or_else(|_| {
            jni_bridge::clear_pending_exception(ctx.env);
            true
        });
    if is_air {
        let _ = ctx.env.delete_local_ref(state);
        let _ = ctx.env.delete_local_ref(block_pos);
        return false;
    }

    
    let solid = ctx
        .env
        .call_method(
            &state,
            "isCollisionShapeFullBlock",
            "(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z",
            &[JValue::Object(&ctx.level), JValue::Object(&block_pos)],
        )
        .and_then(|v| v.z())
        .unwrap_or_else(|_| {
            jni_bridge::clear_pending_exception(ctx.env);
            false
        });

    let _ = ctx.env.delete_local_ref(state);
    let _ = ctx.env.delete_local_ref(block_pos);
    solid
}

fn vec3_from_java(env: &mut jni::JNIEnv<'_>, vec: &JObject<'_>) -> JniResult<Vec3d> {
    Ok(Vec3d {
        x: env.get_field(vec, "x", "D")?.d()?,
        y: env.get_field(vec, "y", "D")?.d()?,
        z: env.get_field(vec, "z", "D")?.d()?,
    })
}



pub fn build_world_geometry(lines: &mut crate::avatar::render::mesh::Mesh, camera: Vec3d) {
    use crate::world_render::geometry;

    for entry in cached_entries() {
        geometry::push_polyline(lines, camera, &entry.points, entry.color);
        if entry.show_hit
            && let Some(hit) = entry.hit_point
        {
            
            
            const ARM: f64 = 0.12;
            for (dx, dy, dz) in [(ARM, 0.0, 0.0), (0.0, ARM, 0.0), (0.0, 0.0, ARM)] {
                geometry::push_segment(
                    lines,
                    camera,
                    Vec3d {
                        x: hit.x - dx,
                        y: hit.y - dy,
                        z: hit.z - dz,
                    },
                    Vec3d {
                        x: hit.x + dx,
                        y: hit.y + dy,
                        z: hit.z + dz,
                    },
                    entry.hit_color,
                );
            }
        }
    }
}
