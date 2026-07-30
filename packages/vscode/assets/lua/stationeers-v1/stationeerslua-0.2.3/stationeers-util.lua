---@meta
-- StationeersLua util.* and json.* namespaces

---@class util
---Utility functions for time, temperature conversion, etc.
util = {}

---@class util.json
---JSON encoding/decoding.
util.json = {}

---Encode a Lua value to a JSON string.
---@param value any Lua table, string, number, boolean, or nil
---@return string json
function util.json.encode(value) end

---Decode a JSON string to a Lua value.
---@param jsonStr string
---@return any value
function util.json.decode(jsonStr) end

---Convert temperature between units.
---@param value number Temperature value
---@param from? string Source unit: "K", "C", or "F" (default "K")
---@param to? string Target unit: "K", "C", or "F" (default "C")
---@return number converted
function util.temp(value, from, to) end

---Get the current game time in seconds since world creation.
---@return number seconds
function util.game_time() end

---Get the number of in-game days that have passed.
---@return number days
function util.days_past() end

---Get the current time of day (0.0 to 1.0).
---@return number fraction
function util.time_of_day() end

---Get the current in-game clock time as a formatted string.
---Supports format tokens: HH (24h), hh (12h), MM/mm (minutes), ss (seconds), A/a (AM/PM).
---@param pattern? string Format pattern (default "HH:MM")
---@return string time e.g. "14:30"
function util.clock_time(pattern) end

-- ── Global json alias ───────────────────────────────────────────────
-- json.encode / json.decode are also available as top-level globals
-- via util.json (same functions, different access path).

---@class json
---Global JSON encoding/decoding (alias for util.json).
json = {}

---Encode a Lua value to a JSON string.
---@param value any
---@return string json
function json.encode(value) end

---Decode a JSON string to a Lua value.
---@param jsonStr string
---@return any value
function json.decode(jsonStr) end

-- ── Global enum shortcuts ───────────────────────────────────────────
-- These are also available via ic.enums.* but commonly used as top-level globals.

---@type table<string, integer>
---LogicType enum values (Temperature, Pressure, Setting, On, etc.)
LogicType = {}

---@type table<string, integer>
---LogicSlotType enum values (Occupied, OccupantHash, Quantity, etc.)
LogicSlotType = {}

---@type table<string, integer>
---LogicBatchMethod enum values (Average, Sum, Minimum, Maximum)
LogicBatchMethod = {}

---@type table<string, integer>
---LogicReagentMode enum values (Contents, Required, Recipe, TotalContents)
LogicReagentMode = {}

-- ── require() for library chips ─────────────────────────────────────

---Load a library module from a library chip on the data network.
---The target chip must have a `--@module <name>` annotation.
---@param modname string Module name matching the --@module annotation
---@return any module The module's return value
function require(modname) end

-- ── print() ───────────────────────────────────────────────────────

---Print values to the in-game Lua console output.
---Multiple arguments are separated by tabs.
---@param ... any Values to print
function print(...) end
