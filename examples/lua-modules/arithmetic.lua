local arithmetic = {}

function arithmetic.clamp(value, minimum, maximum)
    return math.max(minimum, math.min(maximum, value))
end

return arithmetic
