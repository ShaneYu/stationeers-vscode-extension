---@meta
--- Generated from tools/lua_api_profile.json. Editor metadata only.

---@class IcSlot
local IcSlot = {}

---@class IcDevice
local IcDevice = {}

---@class Ic
local ic = {}

---@class DeviceApi
local device = {}

---@param name string
---@return IcDevice
function device.get(name) end

---@param name string
---@return number
function device.getReferenceId(name) end

---@param field string
---@return number
function IcDevice:get(field) end

---@param field string
---@param value number
---@return nil
function IcDevice:set(field, value) end

---@param index number
---@return IcSlot
function IcDevice:slot(index) end

---@param field string
---@return number
function IcSlot:get(field) end

---@param field string
---@param value number
---@return nil
function IcSlot:set(field, value) end

---@param address number
---@return number
function IcDevice:memory(address) end

---@param address number
---@param value number
---@return nil
function IcDevice:setMemory(address, value) end

---@param pin string
---@param field string
---@return number
function ic.get(pin, field) end

---@param pin string
---@param field string
---@param value number
---@return nil
function ic.set(pin, field, value) end

---@param ... any
---@return nil
function print(...) end

---@param ... any
---@return nil
function log(...) end

return { device = device, ic = ic }
