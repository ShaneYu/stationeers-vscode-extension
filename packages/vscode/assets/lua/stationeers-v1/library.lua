---@meta
--- Generated from tools/lua_api_profile.json. Editor metadata only.

---@class IcSlot
local IcSlot = {}

---@class IcDevice
local IcDevice = {}

---@class StationeersDeviceInfo
---@field ref_id number
---@field prefab_hash number
---@field name_hash number
---@field display_name string
local StationeersDeviceInfo = {}

---@class StationeersHostInfo
---@field name string
---@field ref_id number
---@field prefab_hash number
---@field type string
---@field wearer string|nil
local StationeersHostInfo = {}

---@class IcEnums
---@field LogicType table<string, number>
---@field LogicBatchMethod table<string, number>
---@field LogicSlotType table<string, number>
local IcEnums = {}

---@class IcDeviceApi
---@field label fun(deviceIndex: number, name: string): nil
---@field name fun(deviceIndex: number, networkIndex: number): string|nil
local IcDeviceApi = {}

---@class Ic
---@field enums IcEnums
---@field device IcDeviceApi
ic = {}

---@class DeviceApi
device = {}

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

---@param deviceIndex number
---@param logicType number
---@param networkIndex number
---@return number
function ic.read(deviceIndex, logicType, networkIndex) end

---@param deviceIndex number
---@param logicType number
---@param networkIndex number
---@param value number
---@return nil
function ic.write(deviceIndex, logicType, networkIndex, value) end

---@param ... any
---@return nil
function print(...) end

---@param ... any
---@return nil
function log(...) end

return { device = device, ic = ic }
