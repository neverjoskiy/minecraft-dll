

use std::sync::Arc;

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::avatar::lua::state::SharedWorld;
use crate::avatar::math::Vec3;
use crate::avatar::model::ModelPart;
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


pub fn install_models_api(lua: &Lua, world: SharedWorld) -> LuaResult<()> {
    let root = {
        let guard = world
            .lock()
            .map_err(|_| mlua::Error::RuntimeError("script world poisoned".into()))?;
        build_part_table(lua, &world, &[], &guard.models)?
    };
    lua.globals().set("models", root)?;
    Ok(())
}

fn build_part_table(
    lua: &Lua,
    world: &SharedWorld,
    path: &[String],
    part: &ModelPart,
) -> LuaResult<Table> {
    let table = lua.create_table()?;
    install_part_methods(lua, &table, world, path)?;

    for child in &part.children {
        let mut child_path = path.to_vec();
        child_path.push(child.name.clone());
        let child_table = build_part_table(lua, world, &child_path, child)?;
        table.set(child.name.as_str(), child_table)?;
    }
    Ok(table)
}

fn install_part_methods(
    lua: &Lua,
    table: &Table,
    world: &SharedWorld,
    path: &[String],
) -> LuaResult<()> {
    let path_owned: Arc<[String]> = path.to_vec().into();

    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "setRot",
            lua.create_function(move |lua, args: mlua::MultiValue| {
                consume(1.0)?;
                let (x, y, z) = parse_vec3_args(lua, &args)?;
                with_part(&world, &path, |part| {
                    part.rotation = Vec3::new(x, y, z);
                })?;
                
                Ok(Value::Nil)
            })?,
        )?;
        table.set("rot", table.get::<Value>("setRot")?)?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "setPos",
            lua.create_function(move |lua, args: mlua::MultiValue| {
                consume(1.0)?;
                let (x, y, z) = parse_vec3_args(lua, &args)?;
                with_part(&world, &path, |part| {
                    part.position = Vec3::new(x, y, z);
                })?;
                Ok(Value::Nil)
            })?,
        )?;
        table.set("pos", table.get::<Value>("setPos")?)?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "setScale",
            lua.create_function(move |lua, args: mlua::MultiValue| {
                consume(1.0)?;
                let (x, y, z) = parse_scale_args(lua, &args)?;
                with_part(&world, &path, |part| {
                    part.scale = Vec3::new(x, y, z);
                })?;
                Ok(Value::Nil)
            })?,
        )?;
        table.set("scale", table.get::<Value>("setScale")?)?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "setVisible",
            lua.create_function(move |_, args: mlua::MultiValue| {
                consume(0.5)?;
                let visible = last_bool(&args).unwrap_or(true);
                with_part(&world, &path, |part| {
                    part.visible = visible;
                })?;
                Ok(Value::Nil)
            })?,
        )?;
        table.set("visible", table.get::<Value>("setVisible")?)?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "getRot",
            lua.create_function(move |lua, _: mlua::MultiValue| {
                consume(0.25)?;
                let rot = with_part_ret(&world, &path, |part| part.rotation)?;
                vec3_table(lua, rot)
            })?,
        )?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "getPos",
            lua.create_function(move |lua, _: mlua::MultiValue| {
                consume(0.25)?;
                let pos = with_part_ret(&world, &path, |part| part.position)?;
                vec3_table(lua, pos)
            })?,
        )?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "getName",
            lua.create_function(move |_, _: mlua::MultiValue| {
                consume(0.1)?;
                Ok(path.last().cloned().unwrap_or_default())
            })?,
        )?;
    }
    {
        let world = Arc::clone(world);
        let path = Arc::clone(&path_owned);
        table.set(
            "getVisible",
            lua.create_function(move |_, _: mlua::MultiValue| {
                consume(0.1)?;
                with_part_ret(&world, &path, |part| part.visible)
            })?,
        )?;
    }

    Ok(())
}

fn with_part<F: FnOnce(&mut ModelPart)>(
    world: &SharedWorld,
    path: &[String],
    f: F,
) -> LuaResult<()> {
    let mut guard = world
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("script world poisoned".into()))?;
    let part = find_mut(&mut guard.models, path).ok_or_else(|| {
        mlua::Error::RuntimeError(format!("model part not found: {}", path.join(".")))
    })?;
    f(part);
    Ok(())
}

fn with_part_ret<T, F: FnOnce(&ModelPart) -> T>(
    world: &SharedWorld,
    path: &[String],
    f: F,
) -> LuaResult<T> {
    let guard = world
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("script world poisoned".into()))?;
    let part = find_ref(&guard.models, path).ok_or_else(|| {
        mlua::Error::RuntimeError(format!("model part not found: {}", path.join(".")))
    })?;
    Ok(f(part))
}

fn find_mut<'a>(root: &'a mut ModelPart, path: &[String]) -> Option<&'a mut ModelPart> {
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    for name in path {
        let idx = cur.children.iter().position(|c| c.name == *name)?;
        cur = &mut cur.children[idx];
    }
    Some(cur)
}

fn find_ref<'a>(root: &'a ModelPart, path: &[String]) -> Option<&'a ModelPart> {
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    for name in path {
        cur = cur.children.iter().find(|c| c.name == *name)?;
    }
    Some(cur)
}

fn last_bool(args: &mlua::MultiValue) -> Option<bool> {
    for v in args.iter().rev() {
        match v {
            Value::Boolean(b) => return Some(*b),
            _ => {}
        }
    }
    None
}

fn parse_vec3_args(_lua: &Lua, args: &mlua::MultiValue) -> LuaResult<(f64, f64, f64)> {
    
    let vals: Vec<&Value> = args.iter().collect();
    let start = match vals.first() {
        Some(Value::Table(_)) => 1,
        _ => 0,
    };
    let slice = &vals[start.min(vals.len())..];
    if let Some(Value::Table(t)) = slice.first() {
        return Ok((
            t.get::<f64>("x").unwrap_or(0.0),
            t.get::<f64>("y").unwrap_or(0.0),
            t.get::<f64>("z").unwrap_or(0.0),
        ));
    }
    let x = slice.first().and_then(as_f64).unwrap_or(0.0);
    let y = slice.get(1).and_then(as_f64).unwrap_or(0.0);
    let z = slice.get(2).and_then(as_f64).unwrap_or(0.0);
    Ok((x, y, z))
}

fn parse_scale_args(lua: &Lua, args: &mlua::MultiValue) -> LuaResult<(f64, f64, f64)> {
    let (x, y, z) = parse_vec3_args(lua, args)?;
    
    let vals: Vec<&Value> = args.iter().collect();
    let start = match vals.first() {
        Some(Value::Table(_)) => 1,
        _ => 0,
    };
    let nums: Vec<f64> = vals[start..].iter().filter_map(|v| as_f64(v)).collect();
    if nums.len() == 1 {
        let s = nums[0];
        return Ok((s, s, s));
    }
    Ok((x, y, z))
}

fn as_f64(v: &&Value) -> Option<f64> {
    match *v {
        Value::Number(n) => Some(*n),
        Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn vec3_table(lua: &Lua, v: Vec3) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    t.set("z", v.z)?;
    Ok(t)
}
