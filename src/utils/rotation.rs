use jni::{JNIEnv, errors::Result as JniResult, objects::JObject};

use crate::{
    jni_bridge,
    utils::{geometry::Vec3d, minecraft::MinecraftContext},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Angle {
    pub yaw: f64,
    pub pitch: f64,
}

impl Angle {
    pub fn new(yaw: f64, pitch: f64) -> Self {
        Self {
            yaw,
            pitch: pitch.clamp(-90.0, 90.0),
        }
    }
}

pub fn wrap_degrees(value: f64) -> f64 {
    let mut value = value % 360.0;
    if value >= 180.0 {
        value -= 360.0;
    }
    if value < -180.0 {
        value += 360.0;
    }
    value
}

pub fn angle_to_point(eye: Vec3d, point: Vec3d, fallback: Angle) -> Angle {
    let delta = point.subtract(eye);
    let horizontal = (delta.x * delta.x + delta.z * delta.z).sqrt();
    let yaw = if horizontal < 0.001 {
        fallback.yaw
    } else {
        (-delta.x).atan2(delta.z).to_degrees()
    };

    let distance = delta.length();
    let pitch = if distance < 0.001 {
        fallback.pitch
    } else {
        -(delta.y / distance).clamp(-1.0, 1.0).asin().to_degrees()
    };

    Angle::new(yaw, pitch)
}

pub fn mouse_step(ctx: &mut MinecraftContext<'_, '_>) -> JniResult<f64> {
    let sensitivity_option = ctx
        .env
        .call_method(
            &ctx.options,
            "sensitivity",
            "()Lnet/minecraft/client/OptionInstance;",
            &[],
        )?
        .l()?;
    let value = ctx
        .env
        .call_method(&sensitivity_option, "get", "()Ljava/lang/Object;", &[])?
        .l()?;

    let sensitivity = java_number_as_f64(ctx.env, &value).unwrap_or(0.5);
    let base = sensitivity * 0.6 + 0.2;
    let step = base * base * base * 8.0 * 0.15;

    let _ = ctx.env.delete_local_ref(value);
    let _ = ctx.env.delete_local_ref(sensitivity_option);
    Ok(step.max(0.0001))
}

pub fn snap_delta(delta: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return delta;
    }

    delta - (delta % step)
}

fn java_number_as_f64(env: &mut JNIEnv<'_>, value: &JObject<'_>) -> Option<f64> {
    if value.is_null() {
        return None;
    }

    let number_class = jni_bridge::load_class(env, "java.lang.Number").ok()?;
    if !env.is_instance_of(value, &number_class).ok()? {
        return None;
    }

    env.call_method(value, "doubleValue", "()D", &[])
        .ok()?
        .d()
        .ok()
}
