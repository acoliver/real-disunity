//! The yielding functions, written in Lua and loaded like any script, so that a thread asleep
//! in one can be saved and continued.

use super::policy::{Fraction, NegativeSleep, SchedulerPolicy};

/// Yields the token and a whole number of milliseconds, or -1 to wait until woken.
const SLEEP: &str = r#"
local yield, token = ...
local function sleep(ms)
    ms = ms or 0
    if type(ms) ~= "number" or ms ~= ms or ms < 0 then
        error("sleep time must be a non-negative number of milliseconds", 2)
    end
    local ticks = -1
    if ms ~= math.huge then
        ticks = math.tointeger(math.ceil(ms))
    end
    yield(token, ticks)
end
return sleep
"#;

/// Yields the token, a whole number of milliseconds or -1, and the wait kind. The thread is
/// resumed with the values of the wake, or the timeout value of the kind.
const WAIT: &str = r#"
local yield, token, kind = ...
local function wait(timeout)
    local ticks = -1
    if type(timeout) == "number" and timeout >= 0 and timeout ~= math.huge then
        ticks = math.tointeger(math.ceil(timeout)) or -1
    end
    return yield(token, ticks, kind)
end
return wait
"#;

/// The chunk name of the sleep function, which saves refer to.
pub(crate) const SLEEP_CHUNK: &str = "=scheduler";

/// The chunk name of the wait functions.
pub(crate) const WAIT_CHUNK: &str = "=scheduler wait";

/// The sleep function's text under `policy`. The default policy gives [`SLEEP`] unchanged, so
/// saves made before policies still match it.
pub(crate) fn sleep_text(policy: &SchedulerPolicy) -> String {
    let mut text = SLEEP.to_owned();
    if policy.negative_sleep == NegativeSleep::Forever {
        text = text.replace("ms ~= ms or ms < 0", "ms ~= ms").replace(
            "if ms ~= math.huge then",
            "if ms ~= math.huge and ms >= 0 then",
        );
    }
    cut(&text, policy.fraction)
}

pub(crate) fn wait_text(policy: &SchedulerPolicy) -> String {
    cut(WAIT, policy.fraction)
}

fn cut(text: &str, fraction: Fraction) -> String {
    match fraction {
        Fraction::Ceil => text.to_owned(),
        Fraction::Truncate => text.replace("math.ceil", "math.floor"),
    }
}
