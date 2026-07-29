local supplier = require("supplier_logic")

assert(supplier.vendor_for(-1301215609) == "iron-vendor")
assert(supplier.vendor_for(226410516) == "gold-vendor")
assert(supplier.vendor_for(123456) == nil)
print("supplier decision module passed")
