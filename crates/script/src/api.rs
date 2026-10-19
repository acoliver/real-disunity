//! The installable API surface. Hosts compose `worldrunner.*` modules; the
//! host checks each module's declared version against what it actually
//! installed at install time.

use mlua::Lua;

use crate::ScriptError;

/// A `worldrunner.<name>` module. `install` creates the module table under
/// the `worldrunner` global; the host then queries the module's own
/// `api_version()` and compares it with [`Self::api_version`], so a module
/// cannot claim one version and implement another.
pub trait ScriptApi: Send + Sync {
    /// The module name, e.g. `"world"` for `worldrunner.world`.
    fn name(&self) -> &'static str;
    /// The API version this module implements, checked at install time.
    fn api_version(&self) -> (u32, u32, u32);

    fn install(&self, lua: &Lua) -> Result<(), ScriptError>;
}
