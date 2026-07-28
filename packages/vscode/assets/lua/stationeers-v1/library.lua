---@meta
--- Generated Stationeers editor API profile v1. This file is editor metadata only.
---@class IcDevice
local device = {}
---@param name string
---@return IcDevice
function device.get(name) end
---@param name string
---@return number
function device.getReferenceId(name) end
---@class Ic
local ic = {}
---@param name string
---@return number
function ic.get(name) end
---@param name string
---@param value number|string|boolean
function ic.set(name, value) end
---@param callback fun()
function ic.onTick(callback) end
return { device = device, ic = ic }
