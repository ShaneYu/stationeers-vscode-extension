---@meta
-- Narrow `os` surface for StationeersLua chips (LuaChipEnvironmentSandbox).
-- LuaLS has builtin `os` disabled via `.luarc.json` in the mod's Assets/LuaLibrary folder.

---@class os
---@field clock fun(): number
---@field date fun(format?: string, time?: number): string|table
---@field difftime fun(t2: number, t1: number): number
---@field time fun(timeTable?: table): number

---Global os table (clock, date, difftime, time only).
---@type os
os = {}
