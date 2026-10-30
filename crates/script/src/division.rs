//! The division function behind [`DivisionSemantics::IntegerTruncating`](crate::config::DivisionSemantics).
//!
//! `lua-resume` prints each binary `/` of a loaded chunk as a call of [`DIVIDE_HOOK`]. The
//! function never yields, so a coroutine is never suspended inside it and a save never meets
//! its frame.

use mlua::{Function, Lua, Value};

/// The global the printed source calls in place of `/`. The `__lr` prefix is reserved by
/// `lua-resume`, so no variable of a game shadows it.
pub(crate) const DIVIDE_HOOK: &str = "__lr_div";

/// The name a save gives the division function.
pub(crate) const DIVIDE_PERMANENT: &str = "division hook";

/// The division function. Two integers give the quotient truncated toward zero, a zero integer
/// divisor raises an error and two floats divide directly. Every other pair goes through stock
/// Lua division, which gives a float or runs a metamethod.
pub(crate) fn create(lua: &Lua) -> mlua::Result<Function> {
    let stock = lua
        .load("local a, b = ... return a / b")
        .set_name("=__lr_div")
        .into_function()?;
    lua.create_function(move |_, (a, b): (Value, Value)| match (a, b) {
        (Value::Integer(_), Value::Integer(0)) => Err(mlua::Error::RuntimeError(
            "attempt to perform integer division by zero".into(),
        )),
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.wrapping_div(y))),
        (Value::Number(x), Value::Number(y)) => Ok(Value::Number(x / y)),
        (a, b) => stock.call((a, b)),
    })
}
