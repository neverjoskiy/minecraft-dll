use jni::{objects::JObject, JNIEnv};
use crate::{jni_bridge, utils::minecraft::MinecraftContext};
use jni::errors::Result as JniResult;


pub struct IdealHit;

impl IdealHit {
    pub fn get_ai_cooldown(ctx: &mut MinecraftContext<'_, '_>) -> f32 {
        
        if let Ok(stack) = ctx.env.call_method(&ctx.player, "getMainHandStack", "()Lnet/minecraft/world/item/ItemStack;", &[]) {
            if let Ok(obj) = stack.l() {
                if obj.is_null() { return 0.9; }
                
                let _ = ctx.env.delete_local_ref(obj);
                return 0.93;
            }
        }
        let _ = jni_bridge::clear_pending_exception(ctx.env);
        0.9
    }

    pub fn can_critical(ctx: &mut MinecraftContext<'_, '_>, _target: &JObject<'_>) -> bool {
        
        let on_ground = ctx.env.call_method(&ctx.player, "onGround", "()Z", &[]).and_then(|v| v.z()).unwrap_or(true);
        if on_ground { return false; }
        let vel_y = ctx.env.call_method(&ctx.player, "getDeltaMovement", "()Lnet/minecraft/world/phys/Vec3;", &[])
            .and_then(|v| v.l())
            .and_then(|vec| ctx.env.get_field(&vec, "y", "D")?.d())
            .unwrap_or(0.0);
        let fall = ctx.env.get_field(&ctx.player, "fallDistance", "F").and_then(|v| v.f()).unwrap_or(0.0);
        !on_ground && vel_y < 0.0 && fall > 0.0
    }

    pub fn is_no_jump_delay_jump_crit_window(_ctx: &mut MinecraftContext<'_, '_>) -> bool { false }
    pub fn is_in_cobweb(_ctx: &mut MinecraftContext<'_, '_>) -> bool { false }
    pub fn cannot_perform_crit(_ctx: &mut MinecraftContext<'_, '_>) -> bool { false }
}
