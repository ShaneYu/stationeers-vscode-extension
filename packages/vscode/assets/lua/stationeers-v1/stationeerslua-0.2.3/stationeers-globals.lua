---@meta
-- StationeersLua global functions
-- These are injected into the Lua environment by the StationeersLua mod.
-- They are shorthand aliases for the ic.* namespace equivalents.

-- ── Logic Read/Write (shorthand for ic.logic.*) ─────────────────────

---Read a logic value from a pin-connected device.
---@param port integer Device pin index (0-5), or ic.const.BASE_UNIT_INDEX for self
---@param logicType integer LogicType enum value (e.g. LogicType.Temperature)
---@param networkIndex? integer Optional network index override
---@return number|nil value The logic value, or nil if device not found
function read(port, logicType, networkIndex) end

---Write a logic value to a pin-connected device.
---@param port integer Device pin index (0-5)
---@param logicType integer LogicType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function write(port, logicType, value, networkIndex) end

---Read a logic value from a device by reference ID.
---@param refId integer Device reference ID
---@param logicType integer LogicType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function read_id(refId, logicType, networkIndex) end

---Write a logic value to a device by reference ID.
---@param refId integer Device reference ID
---@param logicType integer LogicType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function write_id(refId, logicType, value, networkIndex) end

---Read a logic value from a device slot.
---@param port integer Device pin index
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function read_slot(port, slotIndex, logicSlotType, networkIndex) end

---Write a logic value to a device slot.
---@param port integer Device pin index
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function write_slot(port, slotIndex, logicSlotType, value, networkIndex) end

---Read a logic value from a device slot by reference ID.
---@param refId integer Device reference ID
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function read_slot_id(refId, slotIndex, logicSlotType, networkIndex) end

---Write a logic value to a device slot by reference ID.
---@param refId integer Device reference ID
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function write_slot_id(refId, slotIndex, logicSlotType, value, networkIndex) end

---Read a reagent value from a device.
---@param port integer Device pin index
---@param reagentMode integer LogicReagentMode enum value
---@param reagentHash integer Reagent prefab hash
---@param networkIndex? integer Optional network index override
---@return number|nil value
function read_reagent(port, reagentMode, reagentHash, networkIndex) end

---Get the reagent map for a device slot.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
---@return table|nil map Table of reagent hash -> amount
function rmap(port, networkIndex) end

---Batch read an aggregated logic value from all devices of a type.
---@param prefabHash integer Device prefab hash (use hash("DeviceName"))
---@param logicType integer LogicType enum value
---@param batchMethod integer LogicBatchMethod enum value (Average, Sum, Min, Max)
---@param networkIndex? integer Optional network index override
---@return number value
function batch_read(prefabHash, logicType, batchMethod, networkIndex) end

---Batch read by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param logicType integer LogicType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function batch_read_name(prefabHash, nameHash, logicType, batchMethod, networkIndex) end

---Batch read a slot value from all devices of a type.
---@param prefabHash integer Device prefab hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function batch_read_slot(prefabHash, slotIndex, logicSlotType, batchMethod, networkIndex) end

---Batch read a slot value by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function batch_read_slot_name(prefabHash, nameHash, slotIndex, logicSlotType, batchMethod, networkIndex) end

---Batch write a logic value to all devices of a type.
---@param prefabHash integer Device prefab hash
---@param logicType integer LogicType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function batch_write(prefabHash, logicType, value, networkIndex) end

---Batch write by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param logicType integer LogicType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function batch_write_name(prefabHash, nameHash, logicType, value, networkIndex) end

---Batch write a slot value to all devices of a type.
---@param prefabHash integer Device prefab hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function batch_write_slot(prefabHash, slotIndex, logicSlotType, value, networkIndex) end

---Batch write a slot value by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number The value to write
---@param networkIndex? integer Optional network index override
function batch_write_slot_name(prefabHash, nameHash, slotIndex, logicSlotType, value, networkIndex) end

---Find the first device on the network whose DisplayName matches the given name.
---@param name string Device display name or pattern
---@param mode? string|nil Match mode: "auto" (default), "exact", "glob", "regex" (.NET). Omit if the only optional is networkIndex.
---@param networkIndex? integer Optional network pin index; use `find(name, nil, net)` for default mode with explicit net.
---@return integer|nil refId Reference ID of the matching device, or nil
function find(name, mode, networkIndex) end

---Find all devices on the network whose DisplayName matches the given name.
---@param name string Device display name or pattern
---@param mode? string|nil Match mode: "auto" (default), "exact", "glob", "regex" (.NET). Omit if the only optional is networkIndex.
---@param networkIndex? integer Optional network pin index; use `find_all(name, nil, net)` for default mode with explicit net.
---@return integer[] refIds Array of matching reference IDs
function find_all(name, mode, networkIndex) end

-- ── Memory ──────────────────────────────────────────────────────────

---Read from the chip's internal memory register.
---@param index integer Register index (0-based)
---@return number value
function mem_read(index) end

---Write to the chip's internal memory register.
---@param index integer Register index (0-based)
---@param value number The value to write
function mem_write(index, value) end

---Clear all internal memory registers to 0.
function mem_clear() end

---Read from a device's memory by pin index.
---@param port integer Device pin index
---@param memIndex integer Memory index
---@param networkIndex? integer Optional network index override
---@return number value
function mem_get(port, memIndex, networkIndex) end

---Write to a device's memory by pin index.
---@param port integer Device pin index
---@param memIndex integer Memory index
---@param value number
---@param networkIndex? integer Optional network index override
function mem_put(port, memIndex, value, networkIndex) end

---Read from a device's memory by reference ID.
---@param refId integer Device reference ID
---@param memIndex integer Memory index
---@param networkIndex? integer Optional network index override
---@return number value
function mem_get_id(refId, memIndex, networkIndex) end

---Write to a device's memory by reference ID.
---@param refId integer Device reference ID
---@param memIndex integer Memory index
---@param value number
---@param networkIndex? integer Optional network index override
function mem_put_id(refId, memIndex, value, networkIndex) end

---Clear a device's memory by pin index.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
function mem_clear_device(port, networkIndex) end

---Clear a device's memory by reference ID.
---@param refId integer Device reference ID
---@param networkIndex? integer Optional network index override
function mem_clear_id(refId, networkIndex) end

-- ── Stack ───────────────────────────────────────────────────────────

---Peek at the top of the chip's internal stack without removing it.
---@return number value
function stack_peek() end

---Pop a value from the chip's internal stack.
---@return number value
function stack_pop() end

---Push a value onto the chip's internal stack.
---@param value number
function stack_push(value) end

---Write a value at a specific stack position.
---@param index integer Stack index
---@param value number
function stack_poke(index, value) end

---Get the stack pointer.
---@return integer sp
function stack_get_sp() end

---Set the stack pointer.
---@param sp integer
function stack_set_sp(sp) end

---Get the return address register.
---@return number ra
function stack_get_ra() end

---Set the return address register.
---@param ra number
function stack_set_ra(ra) end

-- ── Utility ─────────────────────────────────────────────────────────

---Compute the CRC32 hash of a string (same as MIPS hash instruction).
---@param str string
---@return integer hash
function hash(str) end

---Get the prefab name from a prefab hash.
---@param prefabHash integer
---@return string|nil name
function prefab_name(prefabHash) end

---Pack a string into an ASCII6 encoded number.
---@param str string Up to 10 characters
---@return number encoded
function pack_ascii6(str) end

---Unpack an ASCII6 encoded number to a string.
---@param encoded number
---@param signed? boolean If true, use signed interpretation (default false)
---@return string str
function unpack_ascii6(encoded, signed) end

---Strip Unity rich text color tags from a string.
---@param str string
---@return string stripped
function strip_color_tags(str) end

---Convert a double to an int53 (truncate to integer, preserving 53-bit range).
---@param value number
---@param signed? boolean If true, use signed interpretation (default true)
---@return integer int53
function to_int53(value, signed) end

-- ── Bitwise ─────────────────────────────────────────────────────────

---@param a number
---@param b number
---@return number
function bit_and(a, b) end

---@param a number
---@param b number
---@return number
function bit_or(a, b) end

---@param a number
---@param b number
---@return number
function bit_xor(a, b) end

---@param a number
---@param b number
---@return number
function bit_nor(a, b) end

---@param a number
---@return number
function bit_not(a) end

---Shift right logical.
---@param value number
---@param amount integer
---@return number
function bit_srl(value, amount) end

---Shift right arithmetic.
---@param value number
---@param amount integer
---@return number
function bit_sra(value, amount) end

---Shift left logical.
---@param value number
---@param amount integer
---@return number
function bit_sll(value, amount) end

---Bit extract.
---@param value number
---@param start integer
---@param count integer
---@return number
function bit_ext(value, start, count) end

---Bit insert.
---@param value number
---@param bits number
---@param start integer
---@param count integer
---@return number
function bit_ins(value, bits, start, count) end

-- ── Device Info ─────────────────────────────────────────────────────

---Get a device's display name by pin index.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
---@return string|nil name
function device_name(port, networkIndex) end

---Set a device's label (custom name) by pin index.
---@param port integer Device pin index
---@param label string Label text to set
function device_label(port, label) end

---Get the display name of a device by prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param networkIndex? integer Optional network index override
---@return string|nil name
function namehash_name(prefabHash, nameHash, networkIndex) end

---List all devices on the data network.
---@param networkIndex? integer Optional network index override
---@return table[] devices Array of {ref_id, display_name, prefab_hash, name_hash}
function device_list(networkIndex) end

---Host device metadata (same as ic.host_info()).
---@return IcHostInfo info
function host_info() end

---Halt and catch fire (stop chip execution permanently).
function hcf() end

---Raise a custom error on the chip housing.
---@param errorCode integer Error state (non-zero = error, 0 = clear)
function raise_error(errorCode) end

---Clear the chip housing error state.
function clear_error() end

-- ── Control Flow ────────────────────────────────────────────────────

---Sleep for a number of seconds (yields the coroutine).
---@param seconds number
function sleep(seconds) end

---Yield execution for one tick.
function yield() end

---Throw a Lua error (alias for error()).
---@param message string
function throw(message) end

-- ── Script callbacks (define in your chip script) ───────────────────

---Called every game tick when the chip host is powered and running.
---@param dt number Elapsed seconds since the last tick.
function tick(dt) end

---Legacy persistence callback (prefer ic.persist). Called on world save and housing power-off before the VM disposes.
---@return string|nil blob String to store (e.g. JSON), or nil.
function serialize() end

---Legacy persistence callback (prefer ic.persist). Called after init on load/recompile.
---ic.persist is hydrated earlier at module level; use ic.persist for new scripts.
---@param data string Blob previously returned by serialize().
function deserialize(data) end
