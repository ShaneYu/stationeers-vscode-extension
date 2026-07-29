//! Deterministic, world-free services for the evidence-backed Lua mock profile.
//!
//! This module deliberately does not pretend to be a Stationeers chip host.
//! It provides the stateful pieces that can be tested without guessing host
//! API names or lifecycle details.  Pure module execution remains owned by
//! [`crate::LuaModuleRunner`].

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

pub const LUA_STATEFUL_MOCK_PROFILE_ID: &str = "stationeerslua-0.9.5.0-lua5.2-stateful-mock-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LuaMockError {
    InvalidClockAdvance,
    Unsupported { capability: String },
}

impl fmt::Display for LuaMockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidClockAdvance => formatter
                .write_str("lua-mock-invalid-clock: advance must be finite and non-negative"),
            Self::Unsupported { capability } => write!(
                formatter,
                "lua-mock-unsupported-api: `{capability}` is unavailable in profile `{LUA_STATEFUL_MOCK_PROFILE_ID}`; no external side effect was performed"
            ),
        }
    }
}

impl std::error::Error for LuaMockError {}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VirtualClock {
    now_seconds: f64,
}

impl VirtualClock {
    pub fn now_seconds(&self) -> f64 {
        self.now_seconds
    }

    pub fn advance(&mut self, seconds: f64) -> Result<f64, LuaMockError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(LuaMockError::InvalidClockAdvance);
        }
        self.now_seconds += seconds;
        Ok(self.now_seconds)
    }

    fn reset(&mut self) {
        self.now_seconds = 0.0;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeterministicRandom {
    seed: u64,
    state: u64,
}

impl DeterministicRandom {
    pub fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        };
        Self { seed, state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        // xorshift64* is small, deterministic, and has no platform-dependent
        // floating-point or system-time behavior.
        let mut value = self.state;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.state = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1_u64 << 53) as f64)
    }

    pub fn reset(&mut self) {
        self.state = self.seed;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PersistedState {
    values: BTreeMap<String, Value>,
}

impl PersistedState {
    pub fn new(values: BTreeMap<String, Value>) -> Self {
        Self { values }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.values.remove(key)
    }

    pub fn snapshot(&self) -> BTreeMap<String, Value> {
        self.values.clone()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lifecycle {
    pub reloads: u64,
    pub power_cycles: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LuaStatefulMock {
    pub state: PersistedState,
    pub clock: VirtualClock,
    pub random: DeterministicRandom,
    pub lifecycle: Lifecycle,
}

impl LuaStatefulMock {
    pub fn new(seed: u64, state: BTreeMap<String, Value>) -> Self {
        Self {
            state: PersistedState::new(state),
            clock: VirtualClock::default(),
            random: DeterministicRandom::new(seed),
            lifecycle: Lifecycle::default(),
        }
    }

    /// Reload the mock VM while retaining persisted key/value state.
    pub fn reload(&mut self) {
        self.lifecycle.reloads += 1;
        self.clock.reset();
        self.random.reset();
    }

    /// Power-cycle the mock while retaining persisted key/value state.
    pub fn power_cycle(&mut self) {
        self.lifecycle.power_cycles += 1;
        self.clock.reset();
        self.random.reset();
    }

    pub fn unsupported(capability: &str) -> LuaMockError {
        LuaMockError::Unsupported {
            capability: capability.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persisted_state_survives_reload_and_power_cycle() {
        let mut mock = LuaStatefulMock::new(7, BTreeMap::new());
        mock.state.set("boot_count", json!(1));
        mock.clock.advance(3.5).unwrap();
        let first_random = mock.random.next_u64();

        mock.reload();
        assert_eq!(mock.state.get("boot_count"), Some(&json!(1)));
        assert_eq!(mock.clock.now_seconds(), 0.0);
        assert_eq!(mock.random.next_u64(), first_random);

        mock.state.set("boot_count", json!(2));
        mock.power_cycle();
        assert_eq!(mock.state.get("boot_count"), Some(&json!(2)));
        assert_eq!(
            mock.lifecycle,
            Lifecycle {
                reloads: 1,
                power_cycles: 1
            }
        );
    }

    #[test]
    fn clock_and_random_are_replayable() {
        let mut first = LuaStatefulMock::new(42, BTreeMap::new());
        let mut second = LuaStatefulMock::new(42, BTreeMap::new());
        assert_eq!(first.clock.advance(0.25), second.clock.advance(0.25));
        assert_eq!(first.random.next_u64(), second.random.next_u64());
        assert_eq!(first.random.next_unit(), second.random.next_unit());
    }

    #[test]
    fn unsupported_extended_capabilities_are_explicit() {
        for capability in ["events", "messaging", "libraryChipRequire", "http"] {
            let error = LuaStatefulMock::unsupported(capability);
            assert!(error.to_string().contains(capability));
            assert!(error.to_string().contains(LUA_STATEFUL_MOCK_PROFILE_ID));
        }
    }

    #[test]
    fn clock_rejects_non_deterministic_input() {
        let mut clock = VirtualClock::default();
        assert_eq!(clock.advance(-1.0), Err(LuaMockError::InvalidClockAdvance));
        assert_eq!(
            clock.advance(f64::NAN),
            Err(LuaMockError::InvalidClockAdvance)
        );
    }
}
