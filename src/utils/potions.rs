use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JString, JValue},
};

use crate::jni_bridge;

use super::{
    minecraft::{MinecraftContext, has_player_and_world},
    name_tags::NameTagPreparedIcon,
};

#[derive(Clone, Debug)]
pub struct PotionEntry {
    pub name: String,
    pub amplifier: i32,
    pub duration_ticks: i32,
    pub color: u32,
    
    pub sprite_id: String,
}

#[derive(Clone, Debug)]
pub struct PotionIconRequest {
    pub sprite_id: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

fn effect_name_from_id(id: &str) -> String {
    let Some(last) = id.rsplit('.').next() else {
        return String::new();
    };
    let mut chars = last.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn collect_active_effects() -> Vec<PotionEntry> {
    let Some(result) = jni_bridge::with_env(|env| {
        let Some(ctx) = MinecraftContext::new(env)? else {
            return Ok(Vec::new());
        };

        let effects = ctx
            .env
            .call_method(
                &ctx.player,
                "getActiveEffects",
                "()Ljava/util/Collection;",
                &[],
            )?
            .l()?;

        if effects.is_null() {
            return Ok(Vec::new());
        }

        let iterator = ctx
            .env
            .call_method(&effects, "iterator", "()Ljava/util/Iterator;", &[])?
            .l()?;

        let mut result = Vec::new();

        while ctx.env.call_method(&iterator, "hasNext", "()Z", &[])?.z()? {
            let instance = ctx
                .env
                .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
                .l()?;

            let duration = ctx
                .env
                .call_method(&instance, "getDuration", "()I", &[])?
                .i()?;
            let amplifier = ctx
                .env
                .call_method(&instance, "getAmplifier", "()I", &[])?
                .i()?;

            let holder = ctx
                .env
                .call_method(&instance, "getEffect", "()Lnet/minecraft/core/Holder;", &[])?
                .l()?;
            let effect = ctx
                .env
                .call_method(&holder, "value", "()Ljava/lang/Object;", &[])?
                .l()?;

            let desc_id = ctx
                .env
                .call_method(&effect, "getDescriptionId", "()Ljava/lang/String;", &[])?
                .l()?;
            let jdesc: JString<'_> = desc_id.into();
            let name: String = ctx.env.get_string(&jdesc)?.into();

            let color = ctx.env.call_method(&effect, "getColor", "()I", &[])?.i()? as u32;

            let sprite_id = mob_effect_sprite_id(ctx.env, &holder).unwrap_or_default();

            result.push(PotionEntry {
                name: effect_name_from_id(&name),
                amplifier,
                duration_ticks: duration,
                color,
                sprite_id,
            });

            let _ = ctx.env.delete_local_ref(jdesc);
            let _ = ctx.env.delete_local_ref(effect);
            let _ = ctx.env.delete_local_ref(holder);
            let _ = ctx.env.delete_local_ref(instance);
        }

        let _ = ctx.env.delete_local_ref(iterator);
        let _ = ctx.env.delete_local_ref(effects);

        Ok(result)
    }) else {
        return Vec::new();
    };

    result
}

pub fn prepare_effect_icons(
    env: &mut JNIEnv<'_>,
    requests: &[PotionIconRequest],
) -> JniResult<Vec<NameTagPreparedIcon>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let Some(mut ctx) = MinecraftContext::new(env)? else {
        return Ok(Vec::new());
    };
    if !has_player_and_world(&ctx) {
        return Ok(Vec::new());
    }

    let atlas_manager = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getAtlasManager",
            "()Lnet/minecraft/client/resources/model/sprite/AtlasManager;",
            &[],
        )?
        .l()?;
    if atlas_manager.is_null() {
        return Ok(Vec::new());
    }

    let gui_atlas_id = identifier_parse(ctx.env, "minecraft:gui")?;
    let atlas = ctx
        .env
        .call_method(
            &atlas_manager,
            "getAtlasOrThrow",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/TextureAtlas;",
            &[JValue::Object(&gui_atlas_id)],
        )?
        .l()?;
    let _ = ctx.env.delete_local_ref(gui_atlas_id);
    let _ = ctx.env.delete_local_ref(atlas_manager);
    if atlas.is_null() {
        return Ok(Vec::new());
    }

    let Some(texture_id) = gpu_texture_gl_id(ctx.env, &atlas)? else {
        let _ = ctx.env.delete_local_ref(atlas);
        return Ok(Vec::new());
    };

    let mut icons = Vec::with_capacity(requests.len());
    for request in requests.iter().take(32) {
        if request.sprite_id.is_empty()
            || !request.x.is_finite()
            || !request.y.is_finite()
            || !request.size.is_finite()
            || request.size <= 1.0
        {
            continue;
        }

        let sprite_loc = match identifier_parse(ctx.env, &request.sprite_id) {
            Ok(loc) => loc,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                continue;
            }
        };
        let sprite = match ctx.env.call_method(
            &atlas,
            "getSprite",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/TextureAtlasSprite;",
            &[JValue::Object(&sprite_loc)],
        ) {
            Ok(value) => value.l()?,
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                let _ = ctx.env.delete_local_ref(sprite_loc);
                continue;
            }
        };
        let _ = ctx.env.delete_local_ref(sprite_loc);
        if sprite.is_null() {
            continue;
        }

        let u0 = ctx.env.call_method(&sprite, "getU0", "()F", &[])?.f()?;
        let u1 = ctx.env.call_method(&sprite, "getU1", "()F", &[])?.f()?;
        let v0 = ctx.env.call_method(&sprite, "getV0", "()F", &[])?.f()?;
        let v1 = ctx.env.call_method(&sprite, "getV1", "()F", &[])?.f()?;
        let _ = ctx.env.delete_local_ref(sprite);

        icons.push(NameTagPreparedIcon {
            texture_id,
            x: request.x.round(),
            y: request.y.round(),
            size: request.size.round(),
            u0,
            v0,
            u1,
            v1,
            nearest_filter: true,
            durability_bar: None,
        });
    }

    let _ = ctx.env.delete_local_ref(atlas);
    Ok(icons)
}

fn mob_effect_sprite_id(env: &mut JNIEnv<'_>, holder: &JObject<'_>) -> JniResult<String> {
    let location = env
        .call_static_method(
            "net/minecraft/client/gui/Hud",
            "getMobEffectSprite",
            "(Lnet/minecraft/core/Holder;)Lnet/minecraft/resources/Identifier;",
            &[JValue::Object(holder)],
        )?
        .l()?;
    if location.is_null() {
        return Ok(String::new());
    }
    let as_string = env
        .call_method(&location, "toString", "()Ljava/lang/String;", &[])?
        .l()?;
    let _ = env.delete_local_ref(location);
    let jstr: JString<'_> = as_string.into();
    let text: String = env.get_string(&jstr)?.into();
    let _ = env.delete_local_ref(jstr);
    Ok(text)
}

fn identifier_parse<'local>(env: &mut JNIEnv<'local>, raw: &str) -> JniResult<JObject<'local>> {
    let java = env.new_string(raw)?;
    let java = JObject::from(java);
    let location = env
        .call_static_method(
            "net/minecraft/resources/Identifier",
            "parse",
            "(Ljava/lang/String;)Lnet/minecraft/resources/Identifier;",
            &[JValue::Object(&java)],
        )?
        .l()?;
    let _ = env.delete_local_ref(java);
    Ok(location)
}

fn gpu_texture_gl_id(
    env: &mut JNIEnv<'_>,
    abstract_texture: &JObject<'_>,
) -> JniResult<Option<u32>> {
    let gpu = match env.call_method(
        abstract_texture,
        "getTexture",
        "()Lcom/mojang/blaze3d/textures/GpuTexture;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    if gpu.is_null() {
        return Ok(None);
    }

    let gl_texture_class = jni_bridge::load_class(env, "com.mojang.blaze3d.opengl.GlTexture")?;
    let id = if env.is_instance_of(&gpu, &gl_texture_class)? {
        env.call_method(&gpu, "glId", "()I", &[])?.i()?
    } else {
        let _ = env.delete_local_ref(gpu);
        return Ok(None);
    };
    let _ = env.delete_local_ref(gpu);
    if id > 0 {
        Ok(Some(id as u32))
    } else {
        Ok(None)
    }
}
