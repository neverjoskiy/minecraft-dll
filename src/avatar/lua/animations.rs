

use std::sync::Arc;

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::avatar::animation::PlayState;
use crate::avatar::lua::state::SharedWorld;
use crate::avatar::permissions;

fn consume(amount: f32) -> LuaResult<()> {
    if permissions::try_consume_complexity_budget(amount, 0.0) {
        Ok(())
    } else {
        Err(mlua::Error::RuntimeError(
            "complexity budget exceeded".into(),
        ))
    }
}

pub fn install_animations_api(lua: &Lua, world: SharedWorld) -> LuaResult<()> {
    let root = lua.create_table()?;
    let model_names: Vec<String> = {
        let guard = world
            .lock()
            .map_err(|_| mlua::Error::RuntimeError("script world poisoned".into()))?;
        guard.animations.keys().cloned().collect()
    };

    for model_name in model_names {
        let model_table = lua.create_table()?;
        let anim_names: Vec<String> = {
            let guard = world
                .lock()
                .map_err(|_| mlua::Error::RuntimeError("script world poisoned".into()))?;
            guard
                .animations
                .get(&model_name)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default()
        };
        for anim_name in anim_names {
            let anim_table = build_anim_table(lua, &world, &model_name, &anim_name)?;
            model_table.set(anim_name.as_str(), anim_table)?;
        }
        
        
        {
            let world = Arc::clone(&world);
            let model_name = model_name.clone();
            model_table.set(
                "stopAll",
                lua.create_function(move |_, _: mlua::MultiValue| {
                    consume(1.0)?;
                    if let Ok(mut guard) = world.lock() {
                        if let Some(map) = guard.animations.get_mut(&model_name) {
                            for anim in map.values_mut() {
                                anim.stop();
                            }
                        }
                    }
                    Ok(())
                })?,
            )?;
        }
        root.set(model_name.as_str(), model_table)?;
    }

    
    {
        let world = Arc::clone(&world);
        root.set(
            "stopAll",
            lua.create_function(move |_, _: mlua::MultiValue| {
                consume(1.0)?;
                if let Ok(mut guard) = world.lock() {
                    for map in guard.animations.values_mut() {
                        for anim in map.values_mut() {
                            anim.stop();
                        }
                    }
                }
                Ok(())
            })?,
        )?;
    }
    {
        let world = Arc::clone(&world);
        root.set(
            "getPlaying",
            lua.create_function(move |lua, _: mlua::MultiValue| {
                consume(0.5)?;
                let t = lua.create_table()?;
                if let Ok(guard) = world.lock() {
                    let mut i = 1;
                    for map in guard.animations.values() {
                        for anim in map.values() {
                            if anim.is_playing() {
                                t.set(i, anim.name.clone())?;
                                i += 1;
                            }
                        }
                    }
                }
                Ok(t)
            })?,
        )?;
    }

    lua.globals().set("animations", root)?;
    Ok(())
}

fn build_anim_table(lua: &Lua, world: &SharedWorld, model: &str, name: &str) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("name", name)?;

    let bind = |method: &'static str| {
        let world = Arc::clone(world);
        let model = model.to_string();
        let name = name.to_string();
        lua.create_function(move |_, _: mlua::MultiValue| {
            consume(1.0)?;
            if let Ok(mut guard) = world.lock() {
                if let Some(anim) = guard
                    .animations
                    .get_mut(&model)
                    .and_then(|m| m.get_mut(&name))
                {
                    match method {
                        "play" => anim.play(),
                        "stop" => anim.stop(),
                        "pause" => anim.pause(),
                        "restart" => {
                            anim.stop();
                            anim.play();
                        }
                        _ => {}
                    }
                }
            }
            Ok(Value::Nil)
        })
    };

    table.set("play", bind("play")?)?;
    table.set("stop", bind("stop")?)?;
    table.set("pause", bind("pause")?)?;
    table.set("restart", bind("restart")?)?;

    {
        let world = Arc::clone(world);
        let model = model.to_string();
        let name = name.to_string();
        table.set(
            "isPlaying",
            lua.create_function(move |_, _: mlua::MultiValue| {
                consume(0.25)?;
                let playing = world
                    .lock()
                    .ok()
                    .and_then(|g| {
                        g.animations
                            .get(&model)
                            .and_then(|m| m.get(&name))
                            .map(|a| a.is_playing())
                    })
                    .unwrap_or(false);
                Ok(playing)
            })?,
        )?;
    }
    {
        let world = Arc::clone(world);
        let model = model.to_string();
        let name = name.to_string();
        table.set(
            "setPlaying",
            lua.create_function(move |_, args: mlua::MultiValue| {
                consume(1.0)?;
                let playing = args
                    .iter()
                    .rev()
                    .find_map(|v| match v {
                        Value::Boolean(b) => Some(*b),
                        _ => None,
                    })
                    .unwrap_or(true);
                if let Ok(mut guard) = world.lock() {
                    if let Some(anim) = guard
                        .animations
                        .get_mut(&model)
                        .and_then(|m| m.get_mut(&name))
                    {
                        if playing {
                            anim.play();
                        } else {
                            anim.stop();
                        }
                    }
                }
                Ok(Value::Nil)
            })?,
        )?;
        table.set("playing", table.get::<Value>("setPlaying")?)?;
    }
    {
        let world = Arc::clone(world);
        let model = model.to_string();
        let name = name.to_string();
        table.set(
            "setBlend",
            lua.create_function(move |_, args: mlua::MultiValue| {
                consume(0.25)?;
                let blend = args.iter().rev().find_map(|v| match v {
                    Value::Number(n) => Some(*n as f32),
                    Value::Integer(i) => Some(*i as f32),
                    _ => None,
                });
                if let Some(blend) = blend {
                    if let Ok(mut guard) = world.lock() {
                        if let Some(anim) = guard
                            .animations
                            .get_mut(&model)
                            .and_then(|m| m.get_mut(&name))
                        {
                            anim.blend = blend;
                        }
                    }
                }
                Ok(Value::Nil)
            })?,
        )?;
    }
    {
        
        table.set(
            "setBlendTime",
            lua.create_function(|_, _: mlua::MultiValue| Ok(Value::Nil))?,
        )?;
    }
    {
        let world = Arc::clone(world);
        let model = model.to_string();
        let name = name.to_string();
        table.set(
            "getPlayState",
            lua.create_function(move |_, _: mlua::MultiValue| {
                consume(0.25)?;
                let state = world
                    .lock()
                    .ok()
                    .and_then(|g| {
                        g.animations
                            .get(&model)
                            .and_then(|m| m.get(&name))
                            .map(|a| match a.play_state {
                                PlayState::Playing => "PLAYING",
                                PlayState::Paused => "PAUSED",
                                PlayState::Stopped => "STOPPED",
                                PlayState::Holding => "HOLDING",
                            })
                    })
                    .unwrap_or("STOPPED");
                Ok(state.to_string())
            })?,
        )?;
    }

    Ok(table)
}
