use jni::{objects::JObject, JNIEnv};
use crate::jni_bridge;
use jni::errors::Result as JniResult;

pub struct HotbarUtil;
impl HotbarUtil {
    pub fn get_axe(env: &mut JNIEnv<'_>, player: &JObject<'_>) -> JniResult<Option<i32>> {
        let inv = env.call_method(player, "getInventory", "()Lnet/minecraft/world/entity/player/Inventory;", &[])?.l()?;
        for slot in 0..9 {
            let stack = env.call_method(&inv, "getItem", "(I)Lnet/minecraft/world/item/ItemStack;", &[jni::objects::JValue::Int(slot)])?.l()?;
            if stack.is_null() { let _ = env.delete_local_ref(stack); continue; }
            let item = env.call_method(&stack, "getItem", "()Lnet/minecraft/world/item/Item;", &[])?.l()?;
            let axe_class = jni_bridge::load_class(env, "net/minecraft/world/item/AxeItem")?;
            let is_axe = env.is_instance_of(&item, &axe_class)?;
            let _ = env.delete_local_ref(item);
            let _ = env.delete_local_ref(stack);
            if is_axe { let _ = env.delete_local_ref(inv); return Ok(Some(slot)); }
        }
        let _ = env.delete_local_ref(inv);
        Ok(None)
    }
}
