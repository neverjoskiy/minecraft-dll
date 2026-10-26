

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use jni::{
    JNIEnv,
    objects::{GlobalRef, JObject, JValue},
    sys::JNI_TRUE,
};

use crate::{avatar::format::BbModel, jni_bridge};

static TEXTURE_IDS: OnceLock<Mutex<HashMap<String, GlobalRef>>> = OnceLock::new();
static TEXTURE_OBJS: OnceLock<Mutex<HashMap<String, GlobalRef>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, GlobalRef>> {
    TEXTURE_IDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn obj_cache() -> &'static Mutex<HashMap<String, GlobalRef>> {
    TEXTURE_OBJS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(avatar_id: &str, tex_index: u32) -> String {
    format!("{avatar_id}:{tex_index}")
}

pub fn ensure_model_textures(
    env: &mut JNIEnv<'_>,
    avatar_id: &str,
    model: &BbModel,
) -> Result<usize, String> {
    let mut registered = 0usize;
    for (index, tex) in model.textures.iter().enumerate() {
        let Some(png) = tex.png_bytes.as_ref() else {
            continue;
        };
        let key = cache_key(avatar_id, index as u32);
        if cache()
            .lock()
            .map(|guard| guard.contains_key(&key))
            .unwrap_or(false)
        {
            continue;
        }
        let id = register_png(env, &key, png)?;
        if let Ok(mut guard) = cache().lock() {
            guard.insert(key, id);
            registered += 1;
        }
    }
    Ok(registered)
}

pub fn has_texture(avatar_id: &str, tex_index: u32) -> bool {
    let key = cache_key(avatar_id, tex_index);
    cache()
        .lock()
        .map(|guard| guard.contains_key(&key))
        .unwrap_or(false)
}

pub fn register_png_for(
    env: &mut JNIEnv<'_>,
    avatar_id: &str,
    tex_index: u32,
    png: &[u8],
) -> Result<(), String> {
    let key = cache_key(avatar_id, tex_index);
    if cache()
        .lock()
        .map(|g| g.contains_key(&key))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let id = register_png(env, &key, png)?;
    if let Ok(mut guard) = cache().lock() {
        guard.insert(key, id);
    }
    Ok(())
}

pub fn texture_identifier<'local>(
    env: &mut JNIEnv<'local>,
    avatar_id: &str,
    tex_index: Option<u32>,
) -> Option<JObject<'local>> {
    let index = tex_index?;
    let key = cache_key(avatar_id, index);
    let Ok(guard) = cache().lock() else {
        return None;
    };
    let global = guard.get(&key)?;
    env.new_local_ref(global.as_obj()).ok()
}

pub fn clear_texture_cache() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = obj_cache().lock() {
        guard.clear();
    }
}

fn register_png(env: &mut JNIEnv<'_>, key: &str, png: &[u8]) -> Result<GlobalRef, String> {
    
    let safe_key: String = key
        .chars()
        .map(|c| match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' | '_' | '-' | '.' => c,
            _ => '_',
        })
        .collect();
    let path = format!("textures/xz_avatar/{safe_key}");
    let identifier = env
        .call_static_method(
            "net/minecraft/resources/Identifier",
            "fromNamespaceAndPath",
            "(Ljava/lang/String;Ljava/lang/String;)Lnet/minecraft/resources/Identifier;",
            &[
                JValue::Object(&env.new_string("xz").map_err(|e| e.to_string())?.into()),
                JValue::Object(&env.new_string(&path).map_err(|e| e.to_string())?.into()),
            ],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let byte_array = env.byte_array_from_slice(png).map_err(|e| e.to_string())?;
    let src_image = env
        .call_static_method(
            "com/mojang/blaze3d/platform/NativeImage",
            "read",
            "([B)Lcom/mojang/blaze3d/platform/NativeImage;",
            &[JValue::Object(&byte_array)],
        )
        .map_err(|e| {
            jni_bridge::clear_pending_exception(env);
            format!("NativeImage.read: {e}")
        })?
        .l()
        .map_err(|e| e.to_string())?;
    if src_image.is_null() {
        return Err("NativeImage.read returned null".into());
    }

    let width = env
        .call_method(&src_image, "getWidth", "()I", &[])
        .map_err(|e| e.to_string())?
        .i()
        .map_err(|e| e.to_string())?;
    let height = env
        .call_method(&src_image, "getHeight", "()I", &[])
        .map_err(|e| e.to_string())?
        .i()
        .map_err(|e| e.to_string())?;
    if width <= 0 || height <= 0 {
        return Err(format!("bad image size {width}x{height}"));
    }

    
    let sample_px = env
        .call_method(
            &src_image,
            "getPixel",
            "(II)I",
            &[JValue::Int(width / 2), JValue::Int(height / 2)],
        )
        .map(|v| v.i().unwrap_or(0))
        .unwrap_or_else(|_| {
            jni_bridge::clear_pending_exception(env);
            0
        });

    
    
    let dynamic = match new_dynamic_from_supplier(env, key, &src_image) {
        Ok(obj) => obj,
        Err(_supplier_err) => new_dynamic_copy_upload(env, key, width, height, &src_image)?,
    };

    
    let _ = env.call_method(&dynamic, "upload", "()V", &[]);
    if env.exception_check().unwrap_or(false) {
        jni_bridge::clear_pending_exception(env);
        return Err("DynamicTexture.upload failed (render-pass?)".into());
    }

    let texture_manager = texture_manager(env)?;
    env.call_method(
        &texture_manager,
        "register",
        "(Lnet/minecraft/resources/Identifier;Lnet/minecraft/client/renderer/texture/AbstractTexture;)V",
        &[JValue::Object(&identifier), JValue::Object(&dynamic)],
    )
    .map_err(|e| {
        jni_bridge::clear_pending_exception(env);
        format!("TextureManager.register: {e}")
    })?;

    
    let tex_keep = env.new_global_ref(&dynamic).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = obj_cache().lock() {
        guard.insert(key.to_string(), tex_keep);
    }

    let a = ((sample_px as u32) >> 24) & 0xff;
    let r = ((sample_px as u32) >> 16) & 0xff;
    let g = ((sample_px as u32) >> 8) & 0xff;
    let b = (sample_px as u32) & 0xff;
    crate::native_jvmti::status::update(|status| {
        status.last_error = format!("texpx {key} {width}x{height} #{r:02x}{g:02x}{b:02x}a{a:02x}");
    });

    env.new_global_ref(&identifier).map_err(|e| e.to_string())
}

fn new_dynamic_from_supplier<'local>(
    env: &mut JNIEnv<'local>,
    key: &str,
    src_image: &JObject<'local>,
) -> Result<JObject<'local>, String> {
    let label = env
        .new_string(format!("xz_avatar/{key}"))
        .map_err(|e| e.to_string())?;
    let supplier = env
        .new_object(
            "xz/runtime/StringSupplier",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&label)],
        )
        .map_err(|e| {
            jni_bridge::clear_pending_exception(env);
            format!("StringSupplier: {e}")
        })?;
    env.new_object(
        "net/minecraft/client/renderer/texture/DynamicTexture",
        "(Ljava/util/function/Supplier;Lcom/mojang/blaze3d/platform/NativeImage;)V",
        &[JValue::Object(&supplier), JValue::Object(src_image)],
    )
    .map_err(|e| {
        jni_bridge::clear_pending_exception(env);
        format!("DynamicTexture(Supplier,NativeImage): {e}")
    })
}

fn new_dynamic_copy_upload<'local>(
    env: &mut JNIEnv<'local>,
    key: &str,
    width: i32,
    height: i32,
    src_image: &JObject<'local>,
) -> Result<JObject<'local>, String> {
    let label = env
        .new_string(format!("xz_avatar/{key}"))
        .map_err(|e| e.to_string())?;
    let dynamic = env
        .new_object(
            "net/minecraft/client/renderer/texture/DynamicTexture",
            "(Ljava/lang/String;IIZ)V",
            &[
                JValue::Object(&label),
                JValue::Int(width),
                JValue::Int(height),
                JValue::Bool(JNI_TRUE),
            ],
        )
        .map_err(|e| {
            jni_bridge::clear_pending_exception(env);
            format!("DynamicTexture(String,w,h): {e}")
        })?;
    let pixels = env
        .call_method(
            &dynamic,
            "getPixels",
            "()Lcom/mojang/blaze3d/platform/NativeImage;",
            &[],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    env.call_method(
        &pixels,
        "copyFrom",
        "(Lcom/mojang/blaze3d/platform/NativeImage;)V",
        &[JValue::Object(src_image)],
    )
    .map_err(|e| {
        jni_bridge::clear_pending_exception(env);
        format!("NativeImage.copyFrom: {e}")
    })?;
    Ok(dynamic)
}

fn texture_manager<'local>(env: &mut JNIEnv<'local>) -> Result<JObject<'local>, String> {
    let minecraft = env
        .call_static_method(
            "net/minecraft/client/Minecraft",
            "getInstance",
            "()Lnet/minecraft/client/Minecraft;",
            &[],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    env.call_method(
        minecraft,
        "getTextureManager",
        "()Lnet/minecraft/client/renderer/texture/TextureManager;",
        &[],
    )
    .map_err(|e| e.to_string())?
    .l()
    .map_err(|e| e.to_string())
}
