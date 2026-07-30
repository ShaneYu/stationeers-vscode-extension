local supplier = require("supplier_logic")
local LT = ic.enums.LogicType
local base = ic.const.BASE_UNIT_INDEX
local requested_item = ic.read(base, LT.Channel0, 0)

local vendor_name = supplier.vendor_for(requested_item)
local vendor = vendor_name and device.get(vendor_name)

if vendor ~= nil then
    vendor:set("Activate", 1)
    ic.write(base, LT.Channel0, 0, 0)
end
