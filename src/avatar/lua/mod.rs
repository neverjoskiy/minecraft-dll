




mod animations;
mod models;
pub mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{
    Error as LuaError, Function, HookTriggers, Lua, MultiValue, Result as LuaResult, Table, Value,
    VmState,
};

use crate::avatar::lua::state::{ScriptWorld, SharedWorld};
use crate::avatar::permissions::{self, PermissionCategory, PermissionDecision};

pub use state::ScriptWorld as LuaScriptWorld;


pub const LUA_VERSION: &str = "5.2";

pub const EVENT_NAMES: &[&str] = &[
    "ENTITY_INIT",
    "TICK",
    "WORLD_TICK",
    "RENDER",
    "POST_RENDER",
    "WORLD_RENDER",
    "POST_WORLD_RENDER",
    "CHAT_SEND_MESSAGE",
    "CHAT_RECEIVE_MESSAGE",
    "SKULL_RENDER",
    "MOUSE_SCROLL",
    "MOUSE_MOVE",
    "MOUSE_PRESS",
    "KEY_PRESS",
    "CHAR_TYPED",
    "USE_ITEM",
    "ARROW_RENDER",
    "TRIDENT_RENDER",
    "ITEM_RENDER",
    "ON_PLAY_SOUND",
    "RESOURCE_RELOAD",
    "TOTEM",
    "DAMAGE",
];

#[derive(Debug, Default)]
struct EventRegistry {
    handlers: HashMap<String, Vec<Function>>,
}

impl EventRegistry {
    fn register(&mut self, name: &str, func: Function) {
        let key = normalize_event_name(name);
        self.handlers.entry(key).or_default().push(func);
    }

    fn call_all(&self, _lua: &Lua, name: &str, args: MultiValue) -> LuaResult<()> {
        let key = normalize_event_name(name);
        let Some(handlers) = self.handlers.get(&key) else {
            return Ok(());
        };
        for handler in handlers {
            handler.call::<MultiValue>(args.clone())?;
        }
        Ok(())
    }
}

fn normalize_event_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}


pub struct AvatarLuaRuntime {
    lua: Lua,
    events: Arc<Mutex<EventRegistry>>,
    world: SharedWorld,
    status: String,
    last_error: Option<String>,
}

impl AvatarLuaRuntime {
    pub fn new(world: SharedWorld) -> Result<Self, String> {
        if permission_denied(PermissionCategory::Script) {
            return Err("script permission denied".to_string());
        }

        let lua = Lua::new();
        if let Err(err) = sandbox(&lua) {
            return Err(format_lua_error("sandbox setup failed", err));
        }
        if let Err(err) = install_instruction_hook(&lua) {
            return Err(format_lua_error("hook setup failed", err));
        }

        let events = Arc::new(Mutex::new(EventRegistry::default()));
        if let Err(err) = install_figura_apis(&lua, Arc::clone(&events), Arc::clone(&world)) {
            return Err(format_lua_error("api setup failed", err));
        }

        Ok(Self {
            lua,
            events,
            world,
            status: "lua runtime ready".to_string(),
            last_error: None,
        })
    }

    pub fn world(&self) -> SharedWorld {
        Arc::clone(&self.world)
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn load_scripts(&mut self, scripts: &[(String, String)]) -> Result<(), String> {
        if permission_denied(PermissionCategory::Script) {
            return Err("script permission denied".to_string());
        }

        for (name, source) in scripts {
            let chunk = self
                .lua
                .load(source.as_str())
                .set_name(name.as_str())
                .set_environment(self.lua.globals());

            match chunk.exec() {
                Ok(()) => {}
                Err(err) => {
                    let message = format!("{name}: {err}");
                    self.status = format!("script error: {message}");
                    self.last_error = Some(message.clone());
                    return Err(message);
                }
            }
        }

        self.status = format!("loaded {} script(s)", scripts.len());
        self.last_error = None;
        Ok(())
    }

    
    pub fn install_require(&mut self, modules: &[(String, String)]) -> Result<(), String> {
        use std::collections::HashMap;
        use std::sync::Mutex as StdMutex;

        let mut by_name: HashMap<String, String> = HashMap::new();
        for (name, source) in modules {
            let stem = name
                .trim_end_matches(".lua")
                .trim_end_matches(".LUA")
                .to_string();
            by_name.insert(stem.to_ascii_lowercase(), source.clone());
            by_name.insert(name.to_ascii_lowercase(), source.clone());
        }
        let store = Arc::new(StdMutex::new(by_name));
        let loaded: Arc<StdMutex<HashMap<String, mlua::Value>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let lua = &self.lua;
        let require_fn = lua
            .create_function(move |lua, module: String| {
                consume_complexity(2.0)?;
                let key = module
                    .trim_end_matches(".lua")
                    .trim_end_matches(".LUA")
                    .to_ascii_lowercase();
                
                
                if matches!(
                    key.as_str(),
                    "gsanimblend"
                        | "squapi"
                        | "squassets"
                        | "physboneapi"
                        | "ezanims"
                        | "ezanims.lua"
                ) {
                    let stub = lua.create_table()?;
                    let mt = lua.create_table()?;
                    mt.set(
                        "__index",
                        lua.create_function(|lua, (_t, _k): (Table, Value)| {
                            
                            let t = lua.create_table()?;
                            let call = lua.create_function(|lua, _: mlua::MultiValue| {
                                Ok(Value::Table(lua.create_table()?))
                            })?;
                            t.set_metatable(Some({
                                let m = lua.create_table()?;
                                m.set("__call", call)?;
                                m
                            }));
                            Ok(Value::Table(t))
                        })?,
                    )?;
                    stub.set_metatable(Some(mt));
                    return Ok(Value::Table(stub));
                }
                if let Ok(cache) = loaded.lock() {
                    if let Some(v) = cache.get(&key) {
                        
                        if matches!(v, Value::Nil) {
                            return Err(LuaError::RuntimeError(format!(
                                "cyclic require('{module}')"
                            )));
                        }
                        return Ok(v.clone());
                    }
                }
                let source = {
                    let guard = store
                        .lock()
                        .map_err(|_| LuaError::RuntimeError("require store poisoned".into()))?;
                    guard.get(&key).cloned().ok_or_else(|| {
                        LuaError::RuntimeError(format!("module '{module}' not found"))
                    })?
                };
                
                if let Ok(mut cache) = loaded.lock() {
                    cache.insert(key.clone(), Value::Nil);
                }
                let chunk = lua
                    .load(source.as_str())
                    .set_name(module.as_str())
                    .set_environment(lua.globals());
                let result = match chunk.eval::<Value>() {
                    Ok(v) => v,
                    Err(err) => {
                        if let Ok(mut cache) = loaded.lock() {
                            cache.remove(&key);
                        }
                        return Err(err);
                    }
                };
                let export = match result {
                    Value::Nil => Value::Boolean(true),
                    other => other,
                };
                if let Ok(mut cache) = loaded.lock() {
                    cache.insert(key, export.clone());
                }
                Ok(export)
            })
            .map_err(|e| format_lua_error("require", e))?;
        lua.globals()
            .set("require", require_fn)
            .map_err(|e| format_lua_error("require global", e))?;
        Ok(())
    }

    pub fn fire_event(&self, name: &str, args: MultiValue) -> Result<(), String> {
        if permission_denied(PermissionCategory::Script) {
            return Err("script permission denied".to_string());
        }

        let registry = self
            .events
            .lock()
            .map_err(|_| "event registry poisoned".to_string())?;

        registry
            .call_all(&self.lua, name, args)
            .map_err(|err| format_lua_error(&format!("event {name}"), err))
    }

    pub fn get_global_string(&self, name: &str) -> Option<String> {
        self.lua
            .globals()
            .get::<Value>(name)
            .ok()
            .and_then(|value| match value {
                Value::String(s) => s.to_str().ok().map(|value| value.to_string()),
                Value::Integer(i) => Some(i.to_string()),
                Value::Number(n) => Some(n.to_string()),
                Value::Boolean(b) => Some(b.to_string()),
                _ => None,
            })
    }

    pub fn is_sandbox_blocked(&self, global_name: &str) -> bool {
        matches!(
            self.lua.globals().get::<Value>(global_name),
            Ok(Value::Nil) | Err(_)
        )
    }
}

fn format_lua_error(context: &str, err: LuaError) -> String {
    format!("{context}: {err}")
}

fn sandbox(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    
    
    for key in ["os", "io", "package", "debug", "dofile", "loadfile"] {
        globals.set(key, Value::Nil)?;
    }
    Ok(())
}

fn install_instruction_hook(lua: &Lua) -> LuaResult<()> {
    lua.set_hook(HookTriggers::new().every_nth_instruction(64), |_, _| {
        if permissions::try_consume_instruction_budget(64.0, 0.0) {
            Ok(VmState::Continue)
        } else {
            Err(LuaError::RuntimeError("instruction budget exceeded".into()))
        }
    });
    Ok(())
}

fn consume_complexity(amount: f32) -> LuaResult<()> {
    if permissions::try_consume_complexity_budget(amount, 0.0) {
        Ok(())
    } else {
        Err(LuaError::RuntimeError("complexity budget exceeded".into()))
    }
}

fn noop_stub(_: &Lua, _: MultiValue) -> LuaResult<MultiValue> {
    consume_complexity(1.0)?;
    Ok(MultiValue::new())
}

fn stub_table(lua: &Lua, methods: &[&str]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for name in methods {
        table.set(*name, lua.create_function(noop_stub)?)?;
    }
    Ok(table)
}

fn permission_denied(category: PermissionCategory) -> bool {
    permissions::check(category) == PermissionDecision::Deny
}

fn install_figura_apis(
    lua: &Lua,
    events: Arc<Mutex<EventRegistry>>,
    world: SharedWorld,
) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set("events", build_events_api(lua, Arc::clone(&events))?)?;
    models::install_models_api(lua, Arc::clone(&world))?;
    animations::install_animations_api(lua, Arc::clone(&world))?;
    globals.set(
        "vanilla_model",
        build_vanilla_model_api(lua, Arc::clone(&world))?,
    )?;
    globals.set("vectors", build_vectors_api(lua)?)?;
    globals.set("matrices", build_matrices_api(lua)?)?;

    
    let secondary: [(&str, &[&str]); 16] = [
        (
            "entity",
            &["getPos", "getRot", "getVelocity", "isCrouching"],
        ),
        ("player", &["getName", "getUUID", "isLoaded"]),
        (
            "world",
            &["getTime", "getMoonPhase", "getEntities", "getBlockState"],
        ),
        ("client", &["isPaused", "getVersion", "isIntegratedServer"]),
        ("host", &["getName", "getUUID", "isHost"]),
        ("renderer", &["getCameraRot"]),
        ("nameplate", &["setText", "setColor"]),
        ("sounds", &["playSound", "stopSound", "isPlaying"]),
        ("particles", &["newParticle", "removeParticle"]),
        ("keybinds", &["newKeybind", "fromVanilla", "getKeybinds"]),
        ("config", &["load", "save", "get", "set"]),
        ("avatar", &["getName", "getAuthors", "getVersion"]),
        ("action_wheel", &["newPage", "setPage", "getPage"]),
        ("data", &["get", "set", "save", "load"]),
        ("json", &["parse", "stringify"]),
        ("pings", &["setPing", "getPing"]),
    ];
    for (name, methods) in secondary {
        globals.set(name, stub_table(lua, methods)?)?;
    }

    globals.set(
        "textures",
        stub_table(
            lua,
            &["newTexture", "fromVanilla", "fromBase64", "getTexture"],
        )?,
    )?;
    globals.set(
        "render_tasks",
        stub_table(
            lua,
            &[
                "newItemTask",
                "newBlockTask",
                "newTextTask",
                "newSpriteTask",
            ],
        )?,
    )?;
    globals.set(
        "metadata",
        stub_table(lua, &["get", "set", "has", "remove"])?,
    )?;

    globals.set("log", lua.create_function(noop_stub)?)?;
    globals.set("print", lua.create_function(noop_stub)?)?;

    
    let vectors: Table = globals.get("vectors")?;
    if let Ok(vec3) = vectors.get::<Function>("vec3") {
        globals.set("vec", vec3)?;
    }

    Ok(())
}


fn build_events_api(lua: &Lua, events: Arc<Mutex<EventRegistry>>) -> LuaResult<Table> {
    let events_table = lua.create_table()?;
    
    
    let slots = lua.create_table()?;

    for event_name in EVENT_NAMES {
        let slot = lua.create_table()?;
        let events_for_slot = Arc::clone(&events);
        let event_key = (*event_name).to_string();

        slot.set(
            "register",
            lua.create_function(move |_, args: MultiValue| {
                consume_complexity(2.0)?;
                let func = args.iter().find_map(|v| match v {
                    Value::Function(f) => Some(f.clone()),
                    _ => None,
                });
                let Some(func) = func else {
                    return Err(LuaError::RuntimeError(
                        "events.EVENT:register expects a function".into(),
                    ));
                };
                if let Ok(mut registry) = events_for_slot.lock() {
                    registry.register(&event_key, func);
                }
                Ok(())
            })?,
        )?;

        slots.set(*event_name, slot.clone())?;
        slots.set(event_name.to_ascii_lowercase(), slot)?;
    }

    let slots_index = slots.clone();
    let events_for_meta = Arc::clone(&events);
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_table, key): (Table, Value)| {
            let name = match key {
                Value::String(s) => s.to_str()?.to_string(),
                _ => return Ok(Value::Nil),
            };
            let up = normalize_event_name(&name);
            if let Ok(v) = slots_index.get::<Value>(up.as_str()) {
                return Ok(v);
            }
            slots_index.get::<Value>(name.to_ascii_lowercase().as_str())
        })?,
    )?;
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_table, key, value): (Table, Value, Value)| {
            let name = match key {
                Value::String(s) => s.to_str()?.to_string(),
                _ => return Ok(()),
            };
            let key_up = normalize_event_name(&name);
            match value {
                Value::Function(func) => {
                    if let Ok(mut registry) = events_for_meta.lock() {
                        registry.register(&key_up, func);
                    }
                }
                Value::Nil => {}
                _ => {
                    return Err(LuaError::RuntimeError(
                        "events.EVENT = value must be a function".into(),
                    ));
                }
            }
            Ok(())
        })?,
    )?;
    events_table.set_metatable(Some(mt));

    Ok(events_table)
}

fn build_vanilla_model_api(lua: &Lua, world: SharedWorld) -> LuaResult<Table> {
    let root = lua.create_table()?;
    let part =
        |lua: &Lua, on_visible: Option<Arc<dyn Fn(bool) + Send + Sync>>| -> LuaResult<Table> {
            let t = lua.create_table()?;
            let cb = on_visible;
            t.set(
                "setVisible",
                lua.create_function(move |_, args: MultiValue| {
                    consume_complexity(0.5)?;
                    let visible = args.iter().rev().find_map(|v| match v {
                        Value::Boolean(b) => Some(*b),
                        _ => None,
                    });
                    if let (Some(v), Some(cb)) = (visible, cb.as_ref()) {
                        cb(v);
                    }
                    Ok(())
                })?,
            )?;
            for method in ["setPos", "setRot", "setScale", "setOffsetRot"] {
                t.set(method, lua.create_function(noop_stub)?)?;
            }
            t.set(
                "getVisible",
                lua.create_function(|_, _: MultiValue| {
                    consume_complexity(0.1)?;
                    Ok(true)
                })?,
            )?;
            Ok(t)
        };

    let player_cb: Arc<dyn Fn(bool) + Send + Sync> = {
        let world = Arc::clone(&world);
        Arc::new(move |visible| {
            if let Ok(mut guard) = world.lock() {
                guard.vanilla_player_visible = Some(visible);
            }
        })
    };

    let groups = [
        "PLAYER",
        "CAPE",
        "ARMOR",
        "OUTER_LAYER",
        "INNER_LAYER",
        "ALL",
        "HEAD",
        "BODY",
        "LEFT_ARM",
        "RIGHT_ARM",
        "LEFT_LEG",
        "RIGHT_LEG",
        "HAT",
        "JACKET",
        "LEFT_SLEEVE",
        "RIGHT_SLEEVE",
        "LEFT_PANTS",
        "RIGHT_PANTS",
        "HELMET",
        "HELMET_ITEM",
        "HELMET_HEAD",
        "CHESTPLATE",
        "LEGGINGS",
        "BOOTS",
        "ELYTRA",
    ];
    for name in groups {
        let cb = if name == "PLAYER" || name == "ALL" {
            Some(Arc::clone(&player_cb))
        } else {
            None
        };
        let p = part(lua, cb)?;
        root.set(name, p.clone())?;
        root.set(name.to_ascii_lowercase(), p)?;
    }
    Ok(root)
}

fn build_vectors_api(lua: &Lua) -> LuaResult<Table> {
    use crate::avatar::math::{Vec2, Vec3, Vec4};

    let table = lua.create_table()?;
    table.set(
        "vec2",
        lua.create_function(|lua, (x, y): (f64, f64)| {
            consume_complexity(0.25)?;
            lua_vec2(lua, Vec2::new(x, y))
        })?,
    )?;
    table.set(
        "vec3",
        lua.create_function(|lua, args: MultiValue| {
            consume_complexity(0.25)?;
            let (x, y, z) = read_vec3_args(&args)?;
            lua_vec3(lua, Vec3::new(x, y, z))
        })?,
    )?;
    table.set(
        "vec4",
        lua.create_function(|lua, (x, y, z, w): (f64, f64, f64, f64)| {
            consume_complexity(0.25)?;
            lua_vec4(lua, Vec4::new(x, y, z, w))
        })?,
    )?;
    
    table.set(
        "vec",
        lua.create_function(|lua, args: MultiValue| {
            consume_complexity(0.25)?;
            let vals: Vec<f64> = args
                .iter()
                .filter_map(|v| match v {
                    Value::Number(n) => Some(*n),
                    Value::Integer(i) => Some(*i as f64),
                    _ => None,
                })
                .collect();
            match vals.as_slice() {
                [x, y] => Ok(Value::Table(lua_vec2(lua, Vec2::new(*x, *y))?)),
                [x, y, z] => Ok(Value::Table(lua_vec3(lua, Vec3::new(*x, *y, *z))?)),
                [x, y, z, w] => Ok(Value::Table(lua_vec4(lua, Vec4::new(*x, *y, *z, *w))?)),
                _ => {
                    let (x, y, z) = read_vec3_args(&args)?;
                    Ok(Value::Table(lua_vec3(lua, Vec3::new(x, y, z))?))
                }
            }
        })?,
    )?;
    table.set("worldToScreenSpace", lua.create_function(noop_stub)?)?;
    table.set("toScreenSpace", lua.create_function(noop_stub)?)?;
    table.set("toCameraSpace", lua.create_function(noop_stub)?)?;
    Ok(table)
}

fn build_matrices_api(lua: &Lua) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set(
        "mat3",
        lua.create_function(|lua, _: ()| {
            consume_complexity(0.25)?;
            lua_mat_identity(lua, 3)
        })?,
    )?;
    table.set(
        "mat4",
        lua.create_function(|lua, _: ()| {
            consume_complexity(0.25)?;
            lua_mat_identity(lua, 4)
        })?,
    )?;
    table.set(
        "identity3",
        lua.create_function(|lua, _: ()| {
            consume_complexity(0.1)?;
            lua_mat_identity(lua, 3)
        })?,
    )?;
    table.set(
        "identity4",
        lua.create_function(|lua, _: ()| {
            consume_complexity(0.1)?;
            lua_mat_identity(lua, 4)
        })?,
    )?;
    table.set("mat2", lua.create_function(noop_stub)?)?;
    table.set("identity2", lua.create_function(noop_stub)?)?;
    Ok(table)
}

fn read_vec3_args(args: &MultiValue) -> LuaResult<(f64, f64, f64)> {
    let vals: Vec<Value> = args.iter().cloned().collect();
    match vals.as_slice() {
        [Value::Table(t)] => Ok((
            t.get::<f64>("x").unwrap_or(0.0),
            t.get::<f64>("y").unwrap_or(0.0),
            t.get::<f64>("z").unwrap_or(0.0),
        )),
        _ => {
            let nums: Vec<f64> = vals
                .iter()
                .filter_map(|v| match v {
                    Value::Number(n) => Some(*n),
                    Value::Integer(i) => Some(*i as f64),
                    _ => None,
                })
                .collect();
            Ok((
                nums.first().copied().unwrap_or(0.0),
                nums.get(1).copied().unwrap_or(0.0),
                nums.get(2).copied().unwrap_or(0.0),
            ))
        }
    }
}

fn lua_vec2(lua: &Lua, v: crate::avatar::math::Vec2) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    Ok(t)
}

fn lua_vec3(lua: &Lua, v: crate::avatar::math::Vec3) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    t.set("z", v.z)?;
    t.set("length", lua.create_function(move |_, ()| Ok(v.length()))?)?;
    Ok(t)
}

fn lua_vec4(lua: &Lua, v: crate::avatar::math::Vec4) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    t.set("z", v.z)?;
    t.set("w", v.w)?;
    Ok(t)
}

fn lua_mat_identity(lua: &Lua, rows: i32) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("identity", true)?;
    t.set("rows", rows)?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::format::{AvatarJson, AvatarPackage, AvatarSource};
    use std::path::PathBuf;

    fn empty_world() -> SharedWorld {
        ScriptWorld {
            models: crate::avatar::model::ModelPart::new_leaf("models"),
            animations: Default::default(),
            vanilla_player_visible: None,
        }
        .share()
    }

    fn test_runtime() -> AvatarLuaRuntime {
        AvatarLuaRuntime::new(empty_world()).expect("runtime")
    }

    #[test]
    fn sandbox_blocks_dangerous_modules() {
        let runtime = test_runtime();
        assert!(runtime.is_sandbox_blocked("io"));
        assert!(runtime.is_sandbox_blocked("os"));
        assert!(runtime.is_sandbox_blocked("package"));
        assert!(runtime.is_sandbox_blocked("debug"));
    }

    #[test]
    fn events_tick_register_and_assignment() {
        let mut runtime = test_runtime();
        runtime
            .load_scripts(&[(
                "main.lua".to_string(),
                r#"
events.TICK:register(function()
    _flag_reg = "a"
end)
events.TICK = function()
    _flag_assign = "b"
end
"#
                .to_string(),
            )])
            .expect("load");
        runtime.fire_event("TICK", MultiValue::new()).expect("tick");
        assert_eq!(runtime.get_global_string("_flag_reg").as_deref(), Some("a"));
        assert_eq!(
            runtime.get_global_string("_flag_assign").as_deref(),
            Some("b")
        );
    }

    #[test]
    fn vector_script_loads() {
        let mut runtime = test_runtime();
        let Ok(source) = std::fs::read_to_string(r"C:\Users\muxam\Desktop\ch33t\vector\script.lua")
        else {
            return;
        };
        runtime
            .load_scripts(&[("script.lua".to_string(), source)])
            .expect("vector script");
    }

    #[test]
    fn models_set_rot_persists() {
        use crate::avatar::model::ModelPart;
        let mut root = ModelPart::new_leaf("models");
        let mut model = ModelPart::new_leaf("model");
        let head = ModelPart::new_group("Head");
        model.children.push(head);
        root.children.push(model);
        let world = ScriptWorld {
            models: root,
            animations: Default::default(),
            vanilla_player_visible: None,
        }
        .share();
        let mut runtime = AvatarLuaRuntime::new(Arc::clone(&world)).expect("rt");
        runtime
            .load_scripts(&[(
                "t.lua".to_string(),
                "models.model.Head:setRot(10, 20, 30)".to_string(),
            )])
            .expect("load");
        let guard = world.lock().unwrap();
        let head = &guard.models.children[0].children[0];
        assert!((head.rotation.x - 10.0).abs() < 1e-6);
        assert!((head.rotation.y - 20.0).abs() < 1e-6);
        assert!((head.rotation.z - 30.0).abs() < 1e-6);
    }

    #[test]
    fn package_world_builds() {
        let _ = AvatarPackage {
            source: AvatarSource::Directory(PathBuf::from(".")),
            json: AvatarJson::default(),
            root: PathBuf::from("."),
            models: vec![],
            metadata_nbt: None,
            avatar_png: None,
        };
    }
}
