use jni::{
    JNIEnv,
    errors::Result as JniResult,
    objects::{JObject, JValue},
};

use crate::{
    jni_bridge,
    utils::{
        minecraft::{MinecraftContext, has_player_and_world},
        name_tags::NameTagPreparedIcon,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct PlayerFaceIconRequest {
    pub entity_id: i32,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}


const UV_HALF_TEXEL: f32 = 0.5 / 64.0;
const FACE_U0: f32 = 8.0 / 64.0 + UV_HALF_TEXEL;
const FACE_V0: f32 = 8.0 / 64.0 + UV_HALF_TEXEL;
const FACE_U1: f32 = 16.0 / 64.0 - UV_HALF_TEXEL;
const FACE_V1: f32 = 16.0 / 64.0 - UV_HALF_TEXEL;
const HAT_U0: f32 = 40.0 / 64.0 + UV_HALF_TEXEL;
const HAT_V0: f32 = 8.0 / 64.0 + UV_HALF_TEXEL;
const HAT_U1: f32 = 48.0 / 64.0 - UV_HALF_TEXEL;
const HAT_V1: f32 = 16.0 / 64.0 - UV_HALF_TEXEL;

pub fn prepare_player_faces(
    env: &mut JNIEnv<'_>,
    requests: &[PlayerFaceIconRequest],
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

    let mut icons = Vec::with_capacity(requests.len() * 2);
    for request in requests.iter().take(16) {
        if !request.x.is_finite()
            || !request.y.is_finite()
            || !request.size.is_finite()
            || request.size <= 1.0
        {
            continue;
        }

        let Some(entity) = ctx.entity_by_id(request.entity_id)? else {
            continue;
        };

        let texture_id = match skin_gl_id(&mut ctx, &entity) {
            Ok(Some(id)) => id,
            Ok(None) => {
                let _ = ctx.env.delete_local_ref(entity);
                continue;
            }
            Err(_) => {
                jni_bridge::clear_pending_exception(ctx.env);
                let _ = ctx.env.delete_local_ref(entity);
                continue;
            }
        };
        let _ = ctx.env.delete_local_ref(entity);
        let x = request.x.round();
        let y = request.y.round();
        let size = request.size.round();

        icons.push(NameTagPreparedIcon {
            texture_id,
            x,
            y,
            size,
            u0: FACE_U0,
            v0: FACE_V0,
            u1: FACE_U1,
            v1: FACE_V1,
            nearest_filter: true,
            durability_bar: None,
        });
        
        icons.push(NameTagPreparedIcon {
            texture_id,
            x,
            y,
            size,
            u0: HAT_U0,
            v0: HAT_V0,
            u1: HAT_U1,
            v1: HAT_V1,
            nearest_filter: true,
            durability_bar: None,
        });
    }

    Ok(icons)
}

fn skin_gl_id<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    entity: &JObject<'local>,
) -> JniResult<Option<u32>> {
    let Some(skin) = resolve_player_skin(ctx, entity)? else {
        return Ok(None);
    };
    let Some(location) = skin_texture_location(ctx.env, &skin)? else {
        let _ = ctx.env.delete_local_ref(skin);
        return Ok(None);
    };
    let _ = ctx.env.delete_local_ref(skin);

    let texture_manager = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getTextureManager",
            "()Lnet/minecraft/client/renderer/texture/TextureManager;",
            &[],
        )?
        .l()?;
    let abstract_texture = ctx
        .env
        .call_method(
            &texture_manager,
            "getTexture",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/AbstractTexture;",
            &[JValue::Object(&location)],
        )?
        .l()?;
    let _ = ctx.env.delete_local_ref(location);
    let _ = ctx.env.delete_local_ref(texture_manager);
    if abstract_texture.is_null() {
        return Ok(None);
    }

    let gl_id = gpu_texture_gl_id(ctx.env, &abstract_texture);
    let _ = ctx.env.delete_local_ref(abstract_texture);
    Ok(gl_id?)
}

fn resolve_player_skin<'local>(
    ctx: &mut MinecraftContext<'_, 'local>,
    entity: &JObject<'local>,
) -> JniResult<Option<JObject<'local>>> {
    
    let skin = match ctx.env.call_method(
        entity,
        "getSkin",
        "()Lnet/minecraft/world/entity/player/PlayerSkin;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            JObject::null()
        }
    };
    if !skin.is_null() {
        return Ok(Some(skin));
    }

    
    let uuid = ctx
        .env
        .call_method(entity, "getUUID", "()Ljava/util/UUID;", &[])?
        .l()?;
    if uuid.is_null() {
        return Ok(None);
    }

    let connection = ctx
        .env
        .call_method(
            &ctx.minecraft,
            "getConnection",
            "()Lnet/minecraft/client/multiplayer/ClientPacketListener;",
            &[],
        )?
        .l()?;
    if connection.is_null() {
        let skin = default_player_skin(ctx.env, &uuid);
        let _ = ctx.env.delete_local_ref(uuid);
        return skin;
    }

    let info = match ctx.env.call_method(
        &connection,
        "getPlayerInfo",
        "(Ljava/util/UUID;)Lnet/minecraft/client/multiplayer/PlayerInfo;",
        &[JValue::Object(&uuid)],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            let _ = ctx.env.delete_local_ref(connection);
            let skin = default_player_skin(ctx.env, &uuid);
            let _ = ctx.env.delete_local_ref(uuid);
            return skin;
        }
    };
    let _ = ctx.env.delete_local_ref(connection);
    if info.is_null() {
        let skin = default_player_skin(ctx.env, &uuid);
        let _ = ctx.env.delete_local_ref(uuid);
        return skin;
    }

    let skin = match ctx.env.call_method(
        &info,
        "getSkin",
        "()Lnet/minecraft/world/entity/player/PlayerSkin;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(ctx.env);
            JObject::null()
        }
    };
    let _ = ctx.env.delete_local_ref(info);
    let result = if skin.is_null() {
        default_player_skin(ctx.env, &uuid)
    } else {
        Ok(Some(skin))
    };
    let _ = ctx.env.delete_local_ref(uuid);
    result
}

fn skin_texture_location<'local>(
    env: &mut JNIEnv<'local>,
    skin: &JObject<'local>,
) -> JniResult<Option<JObject<'local>>> {
    let body = match env.call_method(
        skin,
        "body",
        "()Lnet/minecraft/core/ClientAsset$Texture;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            return Ok(None);
        }
    };
    if body.is_null() {
        return Ok(None);
    }

    let location = match env.call_method(
        &body,
        "texturePath",
        "()Lnet/minecraft/resources/Identifier;",
        &[],
    ) {
        Ok(value) => value.l()?,
        Err(_) => {
            jni_bridge::clear_pending_exception(env);
            let _ = env.delete_local_ref(body);
            return Ok(None);
        }
    };
    let _ = env.delete_local_ref(body);
    Ok((!location.is_null()).then_some(location))
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
            let view = match env.call_method(
                abstract_texture,
                "getTextureView",
                "()Lcom/mojang/blaze3d/textures/GpuTextureView;",
                &[],
            ) {
                Ok(value) => value.l()?,
                Err(_) => {
                    jni_bridge::clear_pending_exception(env);
                    return Ok(None);
                }
            };
            if view.is_null() {
                return Ok(None);
            }
            let texture = env
                .call_method(
                    &view,
                    "texture",
                    "()Lcom/mojang/blaze3d/textures/GpuTexture;",
                    &[],
                )?
                .l()?;
            let _ = env.delete_local_ref(view);
            texture
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

fn default_player_skin<'local>(
    env: &mut JNIEnv<'local>,
    uuid: &JObject<'local>,
) -> JniResult<Option<JObject<'local>>> {
    let default_skin =
        jni_bridge::load_class(env, "net.minecraft.client.resources.DefaultPlayerSkin")?;
    let skin = env
        .call_static_method(
            default_skin,
            "get",
            "(Ljava/util/UUID;)Lnet/minecraft/world/entity/player/PlayerSkin;",
            &[JValue::Object(uuid)],
        )?
        .l()?;
    if skin.is_null() {
        Ok(None)
    } else {
        Ok(Some(skin))
    }
}
