//! The local Lua runtime boundary.
//!
//! Lua is deliberately not embedded yet.  This boundary makes that state
//! explicit so callers do not accidentally treat a desktop Lua interpreter as
//! StationeersLua, while leaving room for a verified Lua 5.2 adapter later.

use std::fmt;
use std::path::{Path, PathBuf};

pub const LUA_PROFILE_ID: &str = "stationeerslua-0.9.5.0-lua5.2";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LuaCapabilityStatus {
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaProfile {
    pub id: &'static str,
    pub lua_version: &'static str,
    pub stationeers_lua_version: &'static str,
    pub runtime: LuaCapabilityStatus,
}

pub const LUA_PROFILE: LuaProfile = LuaProfile {
    id: LUA_PROFILE_ID,
    lua_version: "5.2",
    stationeers_lua_version: "0.9.5.0",
    runtime: LuaCapabilityStatus::Unsupported,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub source_path: PathBuf,
    pub line: Option<usize>,
}

impl fmt::Display for LuaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}: {}",
            self.code,
            self.source_path.display(),
            self.message
        )
    }
}

impl std::error::Error for LuaDiagnostic {}

#[derive(Clone, Copy, Debug, Default)]
pub struct LuaRuntimeBoundary;

impl LuaRuntimeBoundary {
    pub const fn new() -> Self {
        Self
    }

    pub const fn profile(&self) -> &'static LuaProfile {
        &LUA_PROFILE
    }

    /// Return the diagnostic used for every local Lua execution attempt.
    ///
    /// Keeping this operation source-aware gives test runners and editors a
    /// stable place to attach a source location without reading or executing
    /// the source in an unverified runtime.
    pub fn unsupported(&self, program_id: &str, source_path: &Path) -> LuaDiagnostic {
        LuaDiagnostic {
            code: "lua-runtime-unavailable",
            message: format!(
                "unsupported runtime: Lua program `{program_id}` requires profile `{}` (Lua {} / StationeersLua {}), but the local runtime is not enabled; no source was executed",
                LUA_PROFILE.id, LUA_PROFILE.lua_version, LUA_PROFILE.stationeers_lua_version
            ),
            source_path: source_path.to_owned(),
            line: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_is_fail_closed_and_source_aware() {
        let diagnostic = LuaRuntimeBoundary::new().unsupported("controller", Path::new("main.lua"));
        assert_eq!(diagnostic.code, "lua-runtime-unavailable");
        assert_eq!(diagnostic.source_path, Path::new("main.lua"));
        assert!(diagnostic.message.contains("no source was executed"));
        assert_eq!(LuaRuntimeBoundary::new().profile().lua_version, "5.2");
    }
}
