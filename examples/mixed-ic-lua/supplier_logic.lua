local supplier = {}

local vendors = {
    [-1301215609] = "iron-vendor",
    [226410516] = "gold-vendor",
}

function supplier.vendor_for(requested_item)
    return vendors[requested_item]
end

return supplier
