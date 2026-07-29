local arithmetic = require("arithmetic")

assert(arithmetic.clamp(5, 0, 10) == 5)
assert(arithmetic.clamp(-2, 0, 10) == 0)
assert(arithmetic.clamp(12, 0, 10) == 10)
print("arithmetic module passed")
