---@meta
-- StationeersLua ic.* namespace
-- The primary API surface for interacting with Stationeers game systems.

---@class ic
---The main Stationeers Lua API namespace.
ic = {}

-- ── ic.logic ────────────────────────────────────────────────────────

---@class ic.logic
---Logic read/write operations on pin-connected and network devices.
ic.logic = {}

---Read a logic value from a pin-connected device.
---@param port integer Device pin index (0-5)
---@param logicType integer LogicType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function ic.logic.read(port, logicType, networkIndex) end

---Write a logic value to a pin-connected device.
---@param port integer Device pin index (0-5)
---@param logicType integer LogicType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.write(port, logicType, value, networkIndex) end

---Read a logic value by device reference ID.
---@param refId integer Device reference ID
---@param logicType integer LogicType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function ic.logic.read_id(refId, logicType, networkIndex) end

---Write a logic value by device reference ID.
---@param refId integer Device reference ID
---@param logicType integer LogicType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.write_id(refId, logicType, value, networkIndex) end

---Read a slot value from a pin-connected device.
---@param port integer Device pin index
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function ic.logic.read_slot(port, slotIndex, logicSlotType, networkIndex) end

---Write a slot value to a pin-connected device.
---@param port integer Device pin index
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.write_slot(port, slotIndex, logicSlotType, value, networkIndex) end

---Read a slot value by device reference ID.
---@param refId integer Device reference ID
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param networkIndex? integer Optional network index override
---@return number|nil value
function ic.logic.read_slot_id(refId, slotIndex, logicSlotType, networkIndex) end

---Write a slot value by device reference ID.
---@param refId integer Device reference ID
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.write_slot_id(refId, slotIndex, logicSlotType, value, networkIndex) end

---Read a reagent value.
---@param port integer Device pin index
---@param reagentMode integer LogicReagentMode enum value
---@param reagentHash integer Reagent prefab hash
---@param networkIndex? integer Optional network index override
---@return number|nil value
function ic.logic.read_reagent(port, reagentMode, reagentHash, networkIndex) end

---Get the reagent map for a device slot.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
---@return table|nil map Table of reagent hash -> amount
function ic.logic.rmap(port, networkIndex) end

---Batch read an aggregated value from all devices of a type.
---@param prefabHash integer Device prefab hash
---@param logicType integer LogicType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function ic.logic.batch_read(prefabHash, logicType, batchMethod, networkIndex) end

---Batch read by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param logicType integer LogicType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function ic.logic.batch_read_name(prefabHash, nameHash, logicType, batchMethod, networkIndex) end

---Batch read a slot value from all devices of a type.
---@param prefabHash integer Device prefab hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function ic.logic.batch_read_slot(prefabHash, slotIndex, logicSlotType, batchMethod, networkIndex) end

---Batch read a slot value by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param batchMethod integer LogicBatchMethod enum value
---@param networkIndex? integer Optional network index override
---@return number value
function ic.logic.batch_read_slot_name(prefabHash, nameHash, slotIndex, logicSlotType, batchMethod, networkIndex) end

---Batch write a logic value to all devices of a type.
---@param prefabHash integer Device prefab hash
---@param logicType integer LogicType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.batch_write(prefabHash, logicType, value, networkIndex) end

---Batch write by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param logicType integer LogicType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.batch_write_name(prefabHash, nameHash, logicType, value, networkIndex) end

---Batch write a slot value to all devices of a type.
---@param prefabHash integer Device prefab hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.batch_write_slot(prefabHash, slotIndex, logicSlotType, value, networkIndex) end

---Batch write a slot value by device prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param slotIndex integer Slot index
---@param logicSlotType integer LogicSlotType enum value
---@param value number
---@param networkIndex? integer Optional network index override
function ic.logic.batch_write_slot_name(prefabHash, nameHash, slotIndex, logicSlotType, value, networkIndex) end

---Find the first device on the network whose DisplayName (set by the Labeler tool)
---matches the given name. Returns its reference ID for use with ic.read_id / ic.write_id,
---or nil if not found.
---@param name string Device display name or pattern.
---@param mode? string|nil "auto" (default), "exact", "glob", or "regex" (.NET). Omit if the only optional is networkIndex (a number).
---@param networkIndex? integer Optional network pin index (default: search all). Use `ic.find(name, nil, net)` for default mode with explicit net.
---@return integer|nil refId
function ic.logic.find(name, mode, networkIndex) end

---Find all devices on the network whose DisplayName matches the given name.
---Returns a 1-indexed table of reference IDs, or an empty table if none found.
---@param name string Device display name or pattern.
---@param mode? string|nil "auto" (default), "exact", "glob", or "regex" (.NET). Omit if the only optional is networkIndex (a number).
---@param networkIndex? integer Optional network pin index (default: search all). Use `ic.find_all(name, nil, net)` for default mode with explicit net.
---@return integer[] refIds
function ic.logic.find_all(name, mode, networkIndex) end

-- ── ic.device ───────────────────────────────────────────────────────

---@class ic.device
---Device information and memory access.
ic.device = {}

---Get a device's display name by pin index.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
---@return string|nil name
function ic.device.name(port, networkIndex) end

---Set a device's label (custom name) by pin index.
---@param port integer Device pin index
---@param label string Label text to set
function ic.device.label(port, label) end

---Get the display name of a device by prefab hash + name hash.
---@param prefabHash integer Device prefab hash
---@param nameHash integer Device name hash
---@param networkIndex? integer Optional network index override
---@return string|nil name
function ic.device.namehash_name(prefabHash, nameHash, networkIndex) end

---List all devices on the data cable network.
---@param networkIndex? integer Optional network index override
---@return table[] devices Array of {ref_id, display_name, prefab_hash, name_hash}
function ic.device.list(networkIndex) end

---@class IcHostInfo
---@field name string Host display name.
---@field ref_id integer Host reference ID.
---@field prefab_hash integer Host prefab hash.
---@field type "suit"|"scripted_visor"|"circuit_housing"|"tablet"|"motherboard"|"device"|"unknown"
---@field wearer? string Equipped player display name (suit or ScriptedScreens programmable visor).
---@field wearer_ref_id? integer Wearing Human reference ID when resolvable.
---@field wearer_prefab_hash? integer Wearing Human prefab hash when resolvable.

---Metadata for the chip's physical host (suit, housing, tablet, motherboard, device, scripted visor, etc.).
---@return IcHostInfo info
function ic.device.host_info() end

-- ── ic.mem ──────────────────────────────────────────────────────────

---@class ic.mem
---Internal chip memory register access.
ic.mem = {}

---Read from a chip memory register.
---@param index integer Register index
---@return number value
function ic.mem.read(index) end

---Write to a chip memory register.
---@param index integer Register index
---@param value number
function ic.mem.write(index, value) end

---Clear all chip memory registers.
function ic.mem.clear() end

---Read from a device's memory by pin index.
---@param port integer Device pin index
---@param memIndex integer Memory index
---@param networkIndex? integer Optional network index override
---@return number value
function ic.mem.get(port, memIndex, networkIndex) end

---Write to a device's memory by pin index.
---@param port integer Device pin index
---@param memIndex integer Memory index
---@param value number
---@param networkIndex? integer Optional network index override
function ic.mem.put(port, memIndex, value, networkIndex) end

---Read from a device's memory by reference ID.
---@param refId integer Device reference ID
---@param memIndex integer Memory index
---@param networkIndex? integer Optional network index override
---@return number value
function ic.mem.get_id(refId, memIndex, networkIndex) end

---Write to a device's memory by reference ID.
---@param refId integer Device reference ID
---@param memIndex integer Memory index
---@param value number
---@param networkIndex? integer Optional network index override
function ic.mem.put_id(refId, memIndex, value, networkIndex) end

---Clear a device's memory by pin index.
---@param port integer Device pin index
---@param networkIndex? integer Optional network index override
function ic.mem.clear_device(port, networkIndex) end

---Clear a device's memory by reference ID.
---@param refId integer Device reference ID
---@param networkIndex? integer Optional network index override
function ic.mem.clear_id(refId, networkIndex) end

-- ── ic.stack ────────────────────────────────────────────────────────

---@class ic.stack
---Internal chip stack operations.
ic.stack = {}

---Peek at the top of the stack without removing it.
---@return number value
function ic.stack.peek() end

---Pop a value from the stack.
---@return number value
function ic.stack.pop() end

---Push a value onto the stack.
---@param value number
function ic.stack.push(value) end

---Write a value at a specific stack position.
---@param index integer Stack index
---@param value number
function ic.stack.poke(index, value) end

---Get the stack pointer.
---@return integer sp
function ic.stack.get_sp() end

---Set the stack pointer.
---@param sp integer
function ic.stack.set_sp(sp) end

---Get the return address register.
---@return number ra
function ic.stack.get_ra() end

---Set the return address register.
---@param ra number
function ic.stack.set_ra(ra) end

-- ── ic.bit ──────────────────────────────────────────────────────────

---@class ic.bit
---Bitwise operations.
---Note: `and`, `or`, `not`, `xor` are Lua reserved words - access via ic.bit["and"](a, b) etc.
---@field ["and"] fun(a: number, b: number): number Bitwise AND
---@field ["or"] fun(a: number, b: number): number Bitwise OR
---@field ["xor"] fun(a: number, b: number): number Bitwise XOR
---@field ["not"] fun(a: number): number Bitwise NOT
ic.bit = {}

---Bitwise NOR.
---@param a number
---@param b number
---@return number
function ic.bit.nor(a, b) end

---Shift right logical.
---@param value number
---@param amount integer
---@return number
function ic.bit.srl(value, amount) end

---Shift right arithmetic.
---@param value number
---@param amount integer
---@return number
function ic.bit.sra(value, amount) end

---Shift left logical.
---@param value number
---@param amount integer
---@return number
function ic.bit.sll(value, amount) end

---Bit extract.
---@param value number
---@param start integer
---@param count integer
---@return number
function ic.bit.ext(value, start, count) end

---Bit insert.
---@param value number
---@param bits number
---@param start integer
---@param count integer
---@return number
function ic.bit.ins(value, bits, start, count) end

-- ── ic.ascii ────────────────────────────────────────────────────────

---@class ic.ascii
---ASCII6 encoding utilities.
ic.ascii = {}

---Pack a string into ASCII6 encoded number.
---@param str string Up to 10 characters
---@return number encoded
function ic.ascii.pack6(str) end

---Unpack an ASCII6 number to string.
---@param encoded number
---@param signed? boolean If true, use signed interpretation (default false)
---@return string str
function ic.ascii.unpack6(encoded, signed) end

-- ── ic.string ───────────────────────────────────────────────────────

---@class ic.string
---String utilities.
ic.string = {}

---Strip Unity rich text color tags from a string.
---@param str string
---@return string stripped
function ic.string.strip_color_tags(str) end

-- ── ic.persist ──────────────────────────────────────────────────────

---@class ic.persist
---Per-chip string key/value persistence. Survives world save/load, IC housing power off/on
---(VM disposes and recompiles; KV is snapshotted first), and chip pull/reinsert when source
---is unchanged. Keys starting with __ are reserved. Hydrated before module init on load.
---Limits: BepInEx [Lua Persist] MaxKeyLength / MaxValueLength / MaxTotalBytes (defaults 128 / 8192 / 32768).
ic.persist = {}

---Store a string value for this chip.
---@param key string Keys starting with __ are reserved.
---@param value string
---@return boolean ok True on success.
function ic.persist.set(key, value) end

---Read a stored string.
---@param key string
---@return string|nil value Nil when missing.
function ic.persist.get(key) end

---Whether the key exists.
---@param key string
---@return boolean
function ic.persist.has(key) end

---Remove one key.
---@param key string
---@return boolean removed True if the key existed.
function ic.persist.delete(key) end

---Remove all keys for this chip (including legacy serialize blob).
---@return boolean
function ic.persist.clear() end

-- ── ic.events ───────────────────────────────────────────────────────

---@class ic.events
---Event subscription system.
ic.events = {}

---Register an event handler.
---@param event string Event name
---@param handler function|string Callback function or function name
function ic.events.on(event, handler) end

---Unregister an event handler.
---@param event string Event name
function ic.events.off(event) end

-- ── ic.net ──────────────────────────────────────────────────────────

---@class ic.net
---Inter-chip networking (requires AllowNet config).
ic.net = {}

---Get this chip's unique network ID.
---@return integer id
function ic.net.id() end

---Get the data network reference ID.
---@return integer networkId
function ic.net.network_id() end

---Register a listener on a channel. Pass nil as handler to unregister.
---@param channel string|integer Channel name or numeric port (0-1023)
---@param handler function|string|nil Callback function, function name, or nil to unregister
function ic.net.listen(channel, handler) end

---Send a message to a specific chip.
---@param targetId integer|string Target chip network ID or endpoint name
---@param channel string|integer Channel name or numeric port
---@param data any Data to send (must be serializable)
function ic.net.send(targetId, channel, data) end

---Broadcast a message to all chips on the network.
---@param channel string|integer Channel name or numeric port
---@param data any Data to broadcast
---@return integer delivered Number of recipients
function ic.net.broadcast(channel, data) end

---Receive the next pending message (non-blocking). Returns nil if no messages.
---@return integer|nil fromId Sender endpoint ID
---@return string|nil fromName Sender endpoint name
---@return string|integer|nil channel Channel name or numeric port
---@return any data Decoded message payload
function ic.net.recv() end

---Get a list of peer chips on the network.
---@return table[] peers Array of {id=integer, name=string}
function ic.net.peers() end

---Publish a message to a pub/sub topic.
---@param topic string Topic name
---@param data any Data to publish
function ic.net.publish(topic, data) end

---Subscribe to a pub/sub topic. Pass nil as handler to unsubscribe.
---@param topicFilter string Topic name or pattern
---@param handler function|string|nil Callback: function(sender, topic, data) or nil to unsubscribe
function ic.net.subscribe(topicFilter, handler) end

---Unsubscribe from a pub/sub topic.
---@param topicFilter string Topic name or pattern
function ic.net.unsubscribe(topicFilter) end

---Send an RPC request to a remote chip.
---@param targetId integer|string Target chip network ID or endpoint name
---@param method string RPC method name
---@param payload any Request data
---@param callback function|string Callback: function(success, result, error)
---@param timeout? number Timeout in seconds (default 10, max 120)
---@return string requestId Unique request ID for tracking
function ic.net.request(targetId, method, payload, callback, timeout) end

---Register an RPC handler. Pass nil as handler to unregister.
---@param method string RPC method name
---@param handler function|string|nil Handler: function(sender, payload) -> result
function ic.net.register(method, handler) end

---Unregister an RPC handler.
---@param method string RPC method name
function ic.net.unregister(method) end

---Pack a Lua value into a binary string.
---@param value any
---@return string packed
function ic.net.pack(value) end

---Unpack a binary string into a Lua value.
---@param packed string
---@return any value
function ic.net.unpack(packed) end

-- ── ic.wireless ─────────────────────────────────────────────────────

---@class ic.wireless
---Control omni wireless data link when the chip host exposes a wireless provider (suit dev board, wireless tablet cartridge, etc.).
ic.wireless = {}

---List powered transmitters in range. Each entry: id, network_id, name, distance, max_distance.
---@return table[]
function ic.wireless.list() end

---Connect to a transmitter or network id. mesh defaults true (handoff between transmitters on same network).
---@param id number
---@param mesh? boolean
---@return boolean ok, string|nil err On failure returns false and error message.
function ic.wireless.connect(id, mesh) end

---Disconnect from the current wireless target.
function ic.wireless.disconnect() end

---Safe status snapshot; returns { available=false, connected=false } when no wireless device.
---@return table status available, connected, in_range, mesh, network_id, transmitter_id, transmitter_name, distance, max_distance
function ic.wireless.status() end

---Enable or disable mesh handoff without reconnecting.
---@param enabled boolean
function ic.wireless.set_mesh(enabled) end

-- ── ic.http ─────────────────────────────────────────────────────────

---@class ic.http
---HTTP client (requires AllowHttp config). All request methods return a request ID; use poll() to get results.
ic.http = {}

---Send an HTTP GET request.
---@param url string
---@param headers? table<string,string> Optional headers
---@param timeout? number Optional timeout in seconds
---@return integer requestId
function ic.http.get(url, headers, timeout) end

---Send an HTTP POST request.
---@param url string
---@param body string Request body
---@param contentType? string Content-Type header (default "application/json")
---@param headers? table<string,string> Optional headers
---@param timeout? number Optional timeout in seconds
---@return integer requestId
function ic.http.post(url, body, contentType, headers, timeout) end

---Send an HTTP PUT request.
---@param url string
---@param body string Request body
---@param contentType? string Content-Type header
---@param headers? table<string,string> Optional headers
---@param timeout? number Optional timeout in seconds
---@return integer requestId
function ic.http.put(url, body, contentType, headers, timeout) end

---Send an HTTP DELETE request.
---@param url string
---@param headers? table<string,string> Optional headers
---@param timeout? number Optional timeout in seconds
---@return integer requestId
function ic.http.delete(url, headers, timeout) end

---Send an HTTP PATCH request.
---@param url string
---@param body string Request body
---@param contentType? string Content-Type header
---@param headers? table<string,string> Optional headers
---@param timeout? number Optional timeout in seconds
---@return integer requestId
function ic.http.patch(url, body, contentType, headers, timeout) end

---Poll for completed HTTP responses. Returns nil if no pending responses.
---@return integer|nil id Request ID
---@return boolean|nil success Whether the request succeeded
---@return integer|nil statusCode HTTP status code
---@return string|nil body Response body
---@return string|nil error Error message if failed
function ic.http.poll() end

---URL-encode a string.
---@param str string
---@return string encoded
function ic.http.url_encode(str) end

---URL-decode a string.
---@param encoded string
---@return string decoded
function ic.http.url_decode(encoded) end

---Build a URL query string from a table.
---@param params table<string,string|number> Key-value pairs
---@return string query "key1=val1&key2=val2"
function ic.http.build_query(params) end

---Build a full URL by appending query params to a base URL.
---@param base string Base URL
---@param params table<string,string|number> Key-value pairs
---@return string url
function ic.http.build_url(base, params) end

-- ── ic.enums ────────────────────────────────────────────────────────

---@class ic.enums
---Game enum tables. Each field is a table mapping enum name -> integer value.
ic.enums = {}

---@type table<string, integer>
ic.enums.LogicType = {}

---@type table<string, integer>
ic.enums.LogicSlotType = {}

---@type table<string, integer>
ic.enums.LogicBatchMethod = {}

---@type table<string, integer>
ic.enums.LogicReagentMode = {}

---@type table<string, integer>
ic.enums.SoundAlert = {}

---@type table<string, integer>
---Logic Sorter stack instruction opcodes (use with ic.mem_put / ic.mem_get on the sorter).
ic.enums.SorterInstruction = {}

---@type table<string, integer>
---Compare operations used inside packed logic-stack / sorter memory words.
ic.enums.ConditionOperation = {}

-- ── ic.const ────────────────────────────────────────────────────────

---@class ic.const
---Game constants.
ic.const = {}

---Base unit index (-1) - refers to the chip's own housing.
---@type integer
ic.const.BASE_UNIT_INDEX = -1

---Base network index.
---@type integer
ic.const.BASE_NETWORK_INDEX = 0

---First available network index.
---@type integer
ic.const.FIRST_AVAILABLE_NETWORK = 0

-- ── Shorthand aliases on ic table ───────────────────────────────────
-- These are the same functions available as globals and on ic.logic.*

ic.read = ic.logic.read
ic.write = ic.logic.write
ic.read_id = ic.logic.read_id
ic.write_id = ic.logic.write_id
ic.read_slot = ic.logic.read_slot
ic.write_slot = ic.logic.write_slot
ic.read_slot_id = ic.logic.read_slot_id
ic.write_slot_id = ic.logic.write_slot_id
ic.read_reagent = ic.logic.read_reagent
ic.rmap = ic.logic.rmap
ic.batch_read = ic.logic.batch_read
ic.batch_read_name = ic.logic.batch_read_name
ic.batch_read_slot = ic.logic.batch_read_slot
ic.batch_read_slot_name = ic.logic.batch_read_slot_name
ic.batch_write = ic.logic.batch_write
ic.batch_write_name = ic.logic.batch_write_name
ic.batch_write_slot = ic.logic.batch_write_slot
ic.batch_write_slot_name = ic.logic.batch_write_slot_name
ic.find = ic.logic.find
ic.find_all = ic.logic.find_all
ic.host_info = ic.device.host_info
ic.wireless_list = ic.wireless.list
ic.wireless_connect = ic.wireless.connect
ic.wireless_disconnect = ic.wireless.disconnect
ic.wireless_status = ic.wireless.status
ic.wireless_set_mesh = ic.wireless.set_mesh
ic.sleep = sleep
ic.yield = yield
ic.throw = throw
ic.hash = hash
ic.prefab_name = prefab_name
ic.hcf = hcf
ic.raise_error = raise_error
ic.clear_error = clear_error
ic.device_name = ic.device.name
ic.device_label = ic.device.label
ic.device_list = ic.device.list
