//! The engine-provided `bit` table (W3 §6.2 lists `bit` among the engine globals; Lua 5.1 has no
//! bit operations of its own).
//!
//! INFERRED semantics: the common Lua 5.1 "bitop" convention, operating on 32-bit integers. Which
//! functions the original offers and how it converts its float numbers to integers are UNKNOWN; we
//! truncate towards zero and wrap to 32 bits.

use mlua::{Lua, Table};

fn to_i32(n: f64) -> i32 {
    // Truncate, then wrap to 32 bits (so 4294967295 → -1).
    (n.trunc() as i64) as i32
}

/// Builds the `bit` table: band, bor, bxor, bnot, lshift, rshift, arshift.
pub(crate) fn create(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let fold = |lua: &Lua, op: fn(i32, i32) -> i32| {
        lua.create_function(move |_, args: mlua::Variadic<f64>| {
            let mut it = args.iter().map(|&n| to_i32(n));
            let first = it.next().unwrap_or(0);
            Ok(f64::from(it.fold(first, op)))
        })
    };
    t.set("band", fold(lua, |a, b| a & b)?)?;
    t.set("bor", fold(lua, |a, b| a | b)?)?;
    t.set("bxor", fold(lua, |a, b| a ^ b)?)?;
    t.set("bnot", lua.create_function(|_, a: f64| Ok(f64::from(!to_i32(a))))?)?;
    t.set(
        "lshift",
        lua.create_function(|_, (a, n): (f64, f64)| Ok(f64::from(to_i32(a).wrapping_shl(to_i32(n) as u32 & 31))))?,
    )?;
    t.set(
        "rshift",
        lua.create_function(|_, (a, n): (f64, f64)| {
            Ok(f64::from(((to_i32(a) as u32) >> (to_i32(n) as u32 & 31)) as i32))
        })?,
    )?;
    t.set(
        "arshift",
        lua.create_function(|_, (a, n): (f64, f64)| Ok(f64::from(to_i32(a) >> (to_i32(n) as u32 & 31))))?,
    )?;
    Ok(t)
}
