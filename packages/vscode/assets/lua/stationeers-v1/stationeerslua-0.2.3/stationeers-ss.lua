---@meta
-- ScriptedScreens ss.* namespace
-- UI system for creating interactive screens on Stationeers consoles and tablets.

---@class ss
---ScriptedScreens root namespace.
ss = {}

---@class ss.ui
---UI surface management.
ss.ui = {}

---@class ss.hud
---Heads-up UI for chips hosted in Programmable Visor glasses (screen-space overlay). Same surface API shape as `ss.ui` on tablets/boards, but only valid when the chip's parent is visor glasses.
ss.hud = {}

---Get or create a named HUD surface on the visor (chip must be in Programmable Visor glasses).
---@param name string Surface name (e.g. "main")
---@return SsSurface surface
function ss.hud.surface(name) end

---Activate a HUD surface on the visor.
---@param name string Surface name
function ss.hud.activate(name) end

---@class SsHudSafeArea
---@field ok boolean True when an overlay snapshot is available; false falls back to full canvas.
---@field x number Canvas-space X of the safe rect.
---@field y number Canvas-space Y of the safe rect.
---@field w number Width.
---@field h number Height.
---@field pad number Padding applied around vanilla panels (set via ss.hud.set_safe_padding).

---@class SsHudAnchorResult
---@field x number Anchored canvas-space X.
---@field y number Anchored canvas-space Y.
---@field w number Width passed in.
---@field h number Height passed in.
---@field anchor string The anchor name that was selected.

---Compute a "safe to draw" rect inside the visor canvas, with every edge-touching vanilla
---panel from `ss.client_overlay()` already excluded plus a configurable pad. Use this
---directly as your panel rect to keep your HUD off the hands strip / suit status / open
---inventory windows / Stationpedia / etc. - no manual scaling, no Y flip.
---@param pad? number Override safe-area padding (default 8 px or whatever `ss.hud.set_safe_padding` set).
---@return SsHudSafeArea safe Always returns a usable rect; falls back to the full canvas if no snapshot.
function ss.hud.safe_area(pad) end

---True iff any vanilla panel from `ss.client_overlay()` overlaps the given canvas-space
---rect. Useful for "does my proposed placement collide with anything currently on screen?"
---@param x number Canvas-space X.
---@param y number Canvas-space Y.
---@param w number Width.
---@param h number Height.
---@return boolean intersects
function ss.hud.intersects_vanilla(x, y, w, h) end

---Try a list of named anchors inside the safe area and return the first placement that
---doesn't intersect any vanilla panel. Falls back to the first anchor if every option
---conflicts. Anchor names: "top_left", "top_right", "top_center", "bottom_left",
---"bottom_right", "bottom_center", "left", "right", "center".
---@param anchors string[] Ordered list of anchor names to try.
---@param w number Panel width.
---@param h number Panel height.
---@return SsHudAnchorResult placement
function ss.hud.first_free_anchor(anchors, w, h) end

---Override the default safe-area padding (canvas px) used by `ss.hud.safe_area()`.
---Pass nil or a negative value to reset to the built-in default of 8.
---@param pad? number
function ss.hud.set_safe_padding(pad) end

---Register a callback invoked when the vanilla visor overlay changes.
---Use this to rebuild layout without a polling loop.
---@param callback? fun()
function ss.hud.on_overlay_change(callback) end

---Snapshot of every vanilla UI element worth avoiding, in **visor canvas coordinates**
---(top-left origin, Y-down) - the same space `hud:size()`, `hud:layout`, and `lay_dx` /
---`lay_dy` already use, so scripts never have to convert. Prefer the helpers
---`ss.hud.safe_area()` / `ss.hud.first_free_anchor()` over hand-walking `panels[]`.
---@return SsClientOverlayInfo|nil
function ss.client_overlay() end

---@class SsClientOverlayPanel
---@field id string Stable id (e.g. "hands", "clothing", "inventory_window_3", "stationpedia").
---@field kind string Category: "hands"|"clothing"|"vitals"|"inventory_window"|"chrome"|"system_modal"|"panel".
---@field title? string Optional human-readable title (e.g. inventory window slot name + occupant).
---@field x number Canvas-space X (top-left origin, Y down).
---@field y number Canvas-space Y.
---@field w number Width in canvas pixels.
---@field h number Height in canvas pixels.

---@class SsClientOverlayInfo
---@field ok boolean False when no snapshot is available (no wearer client / batch + no relay).
---@field reason? string When ok is false.
---@field version integer Monotonic snapshot version for diagnostics.
---@field source string "local" (this process is the wearer) or "remote_client" (relayed from wearer's game client) or "none".
---@field canvas_w number Visor canvas width (pixels) - same coordinate space as hud:size().
---@field canvas_h number Visor canvas height (pixels).
---@field screen_w number Wearer's raw screen width (pixels) - diagnostic only; scripts should use canvas_w/h.
---@field screen_h number Wearer's raw screen height (pixels) - diagnostic only.
---@field show_menu boolean Esc / game menu open.
---@field show_ui boolean Global HUD shown (false = F3-style hide).
---@field console_open boolean
---@field stationpedia_open boolean
---@field stationpedia_locked boolean
---@field input_modal boolean Text / input modal up.
---@field creative_spawn_menu boolean Creative spawn ImGui menu.
---@field character_customisation boolean Character customisation scene loaded.
---@field panels SsClientOverlayPanel[] Every visible vanilla rect in canvas coords.
---@field relay_age_s number Seconds since the wearer's client last pushed a snapshot (0 in local case).
---@field relay_stale boolean True iff relay_age_s > 5.

---Get or create a named surface for this screen.
---@param name string Surface name (e.g. "main", "settings")
---@param screenIndex? integer Physical screen index (default 0)
---@return SsSurface surface
function ss.ui.surface(name, screenIndex) end

---Activate a surface, making it the visible one.
---@param name string Surface name
function ss.ui.activate(name) end

---Optional hint label for device pin d{index} (0-5), shown on the built-in config screen. Motherboard/circuitboard only.
---@param index integer Pin index 0-5 (same as ic.read/write first argument)
---@param label? string Set label when provided; omit to read current label (returns string or nil)
---@return string?
function ss.pin_label(index, label) end

-- Surface object -------------------------------------------------------

---@class SsSurface
---A UI surface representing one screen page.
local SsSurface = {}

---Create or update a UI element on this surface.
---@param def SsElementDef Element definition table
---@return SsElementHandle handle
function SsSurface:element(def) end

---Remove all elements from this surface.
function SsSurface:clear() end

---Remove a specific element by ID.
---@param id string Element ID
function SsSurface:remove(id) end

---Get a handle to an existing element.
---@param id string Element ID
---@return SsElementHandle handle
function SsSurface:get(id) end

---Flush pending UI changes to clients. Call after creating/updating elements.
function SsSurface:commit() end

---Get the physical screen size.
---@return {w: number, h: number} size
function SsSurface:size() end

---Set a virtual resolution override. Pass 0,0 to revert to physical size.
---@param w number Width
---@param h number Height
function SsSurface:set_resolution(w, h) end

---Get the current effective resolution.
---@return {w: number, h: number} size
function SsSurface:get_resolution() end

---Measure text dimensions before rendering.
---@param text string Text to measure
---@param maxWidth? number Maximum width for wrapping (0 = no limit)
---@param fontSize? integer Font size (default 16)
---@param wrap? boolean Enable word wrapping
---@return {w: number, h: number} size
function SsSurface:measure_text(text, maxWidth, fontSize, wrap) end

---Create a nested layout tree (flex or grid). Returns handles keyed by child ID.
---@param def SsLayoutDef Layout definition table
---@return table<string, SsElementHandle> handles
function SsSurface:layout(def) end

---Register a per-frame callback. Pass nil to unregister.
---@param callback? fun(dt: number)
function SsSurface:on_frame(callback) end

---Register a key-down event handler.
---@param handler fun(key: string, player: string)
function SsSurface:on_keydown(handler) end

---Register a key-up event handler.
---@param handler fun(key: string, player: string)
function SsSurface:on_keyup(handler) end

---Register a callback invoked after a drag on this surface completes.
---The callback receives the dragged element id.
---@param handler? fun(id: string)|string
function SsSurface:on_drag(handler) end

---Get the cumulative stored drag offset for an element id.
---@param id string
---@return {dx: number, dy: number} offset
function SsSurface:drag_offset(id) end

---Overwrite the stored drag offset for an element id.
---@param id string
---@param dx number
---@param dy number
function SsSurface:set_drag_offset(id, dx, dy) end

---Drain and return all pending input events since last call.
---@return SsInputEvent[] events
function SsSurface:poll_input() end

-- Canvas drawing methods -----------------------------------------------

---Clear a canvas element to a solid color.
---@param canvasId string Canvas element ID
---@param r integer Red (0-255)
---@param g integer Green (0-255)
---@param b integer Blue (0-255)
---@param a? integer Alpha (0-255, default 255)
function SsSurface:canvas_clear(canvasId, r, g, b, a) end

---Set a single pixel on a canvas.
---@param canvasId string
---@param x integer
---@param y integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer Alpha (0-255, default 255)
function SsSurface:canvas_pixel(canvasId, x, y, r, g, b, a) end

---Draw a vertical line on a canvas.
---@param canvasId string
---@param x integer
---@param y1 integer
---@param y2 integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
function SsSurface:canvas_vline(canvasId, x, y1, y2, r, g, b, a) end

---Draw a horizontal line on a canvas.
---@param canvasId string
---@param y integer
---@param x1 integer
---@param x2 integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
function SsSurface:canvas_hline(canvasId, y, x1, x2, r, g, b, a) end

---Draw a filled rectangle on a canvas.
---@param canvasId string
---@param x integer
---@param y integer
---@param w integer
---@param h integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
function SsSurface:canvas_rect(canvasId, x, y, w, h, r, g, b, a) end

---Draw a rectangle outline on a canvas.
---@param canvasId string
---@param x integer
---@param y integer
---@param w integer
---@param h integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
---@param thickness? integer Line thickness (default 1)
function SsSurface:canvas_rect_outline(canvasId, x, y, w, h, r, g, b, a, thickness) end

---Draw a line between two points on a canvas.
---@param canvasId string
---@param x1 integer
---@param y1 integer
---@param x2 integer
---@param y2 integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
---@param thickness? integer Line thickness (default 1)
function SsSurface:canvas_line(canvasId, x1, y1, x2, y2, r, g, b, a, thickness) end

---Draw a circle outline on a canvas.
---@param canvasId string
---@param cx integer Center X
---@param cy integer Center Y
---@param radius integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
---@param thickness? integer Line thickness (default 1)
function SsSurface:canvas_circle(canvasId, cx, cy, radius, r, g, b, a, thickness) end

---Draw a filled circle on a canvas.
---@param canvasId string
---@param cx integer Center X
---@param cy integer Center Y
---@param radius integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
function SsSurface:canvas_fill_circle(canvasId, cx, cy, radius, r, g, b, a) end

---Draw a filled triangle on a canvas.
---@param canvasId string
---@param x1 integer
---@param y1 integer
---@param x2 integer
---@param y2 integer
---@param x3 integer
---@param y3 integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
function SsSurface:canvas_fill_triangle(canvasId, x1, y1, x2, y2, x3, y3, r, g, b, a) end

---Draw a polyline (connected line segments) on a canvas.
---@param canvasId string
---@param points integer[] Flat array of x,y pairs: {x1,y1, x2,y2, ...}
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
---@param thickness? integer Line thickness (default 1)
function SsSurface:canvas_polyline(canvasId, points, r, g, b, a, thickness) end

---Draw an arc on a canvas.
---@param canvasId string
---@param cx integer Center X
---@param cy integer Center Y
---@param radius integer
---@param startAngle number Start angle in degrees
---@param endAngle number End angle in degrees
---@param r integer
---@param g integer
---@param b integer
---@param a? integer
---@param thickness? integer Line thickness (default 1)
function SsSurface:canvas_arc(canvasId, cx, cy, radius, startAngle, endAngle, r, g, b, a, thickness) end

---Begin a grouped canvas paint (multiplayer: may flush as one batch). Pair with canvas_end_update.
---@param canvasId string
function SsSurface:canvas_begin_update(canvasId) end

---End a grouped canvas paint started with canvas_begin_update.
---@param canvasId string
function SsSurface:canvas_end_update(canvasId) end

---Run synchronous drawing between begin/end. Callback must not yield.
---@param canvasId string
---@param fn fun()
function SsSurface:canvas_with_update(canvasId, fn) end

---Apply canvas changes (flush pixel buffer to texture).
---@param canvasId string
function SsSurface:canvas_apply(canvasId) end

-- Sound methods (on ss root table, not surface) -------------------------
-- See ss.play_sound, ss.stop_sound, ss.stop_all_sounds below.

-- Element handle -------------------------------------------------------

---@class SsElementHandle
---Handle to a UI element, returned by surface:element() and surface:get().
local SsElementHandle = {}

---Update the element's props (text, value, etc.).
---@param props table
function SsElementHandle:set_props(props) end

---Update the element's style (color, font_size, bg, etc.).
---@param style table
function SsElementHandle:set_style(style) end

---Register an event handler on this element.
---@param event string Event name: "click", "change", "toggle", "interface_mode", "drag_begin", "drag_end", "drop", "drag_payload_begin", or "drag_payload_cancel"
---@param handler function|string|nil Callback function, function name, or nil to unregister
function SsElementHandle:on(event, handler) end

---Unregister an event handler on this element.
---@param event string Event name: "click", "change", "toggle", "interface_mode", "drag_begin", "drag_end", "drop", "drag_payload_begin", or "drag_payload_cancel"
function SsElementHandle:off(event) end

---Create a child element under this element.
---@param def SsElementDef
---@return SsElementHandle
function SsElementHandle:element(def) end

-- Element definition ---------------------------------------------------

---@class SsElementDef
---@field id string Unique element identifier
---@field type string Element type: "label"|"panel"|"button"|"checkbox"|"radio_group"|"progress"|"slider"|"toggle"|"select"|"textinput"|"line"|"rect_outline"|"border"|"circle"|"icon"|"divider"|"scrollview"|"canvas"|"media"|"interface_button"|"sparkline"|"linechart"|"barchart"|"gauge"|"table"|"image"
---@field rect table Position/size: {x, y, w, h} or {unit="px"|"%", x, y, w, h}
---@field props? table Element-specific properties (text, value, color, etc.)
---@field style? table Visual styling (font_size, color, bg, align, etc.)
---@field visible? boolean Whether the element is visible (default true)
---@field parent_id? string Parent element ID for nesting
---@field on_click? fun(value: any, playerRefId: integer) Click handler (buttons)
---@field on_change? fun(value: any, playerRefId: integer) Change handler (sliders, inputs)
---@field on_toggle? fun(value: boolean, playerRefId: integer) Toggle handler (checkboxes, toggles)

-- Layout definition ----------------------------------------------------

---@class SsLayoutDef
---@field layout string "flex"|"grid"
---@field direction? string "column"|"row" (for flex, default "column")
---@field rect table Container rect: {x, y, w, h}
---@field gap? number Gap between children in pixels
---@field padding? number Padding inside container in pixels
---@field columns? integer Number of columns (for grid layout)
---@field children SsLayoutChildDef[] Array of child element definitions

---@class SsLayoutChildDef : SsElementDef
---@field flex? number Flex grow factor (for flex layout)
---@field height? number Fixed height override

-- Input event ----------------------------------------------------------

---@class SsInputEvent
---@field event string Event name ("click", "change", "toggle", "keydown", "keyup")
---@field id string Element ID that triggered the event
---@field value any Event value
---@field player string Player name

-- Tablet API -----------------------------------------------------------

---@class ss.tablet
---Tablet/cartridge API for wireless screen interaction.
ss.tablet = {}

---Subscribe to tablet target detection. Callback fires when player aims at a screen.
---@param callback fun(target: SsTabletTarget|nil) Called with target info or nil
---@param interval? number Poll interval in seconds (default 0.2)
function ss.tablet.target(callback, interval) end

---@class SsTabletTarget
---@field ref_id integer Target device reference ID
---@field display_name string Target device display name
---@field distance number Distance to target

---@class ss.tablet.wireless
---Wireless connection management for tablets.
ss.tablet.wireless = {}

---List available wireless targets.
---@return table[] targets Array of {ref_id, display_name, distance}
function ss.tablet.wireless.list() end

---Connect to a wireless target.
---@param refId integer Target device reference ID
function ss.tablet.wireless.connect(refId) end

---Disconnect from the current wireless target.
function ss.tablet.wireless.disconnect() end

---Get the current wireless connection status.
---@return table|nil status {connected, ref_id, display_name} or nil
function ss.tablet.wireless.status() end

-- Icon and sound enums -------------------------------------------------

---@type table<string, integer>
---Available icon names for icon elements.
ss.icons = {}

---@type table<string, integer>
---Available prefab icon names.
ss.prefab = {}

---@type table<string, integer>
---Available sound effect names for play_sound().
ss.sounds = {}

---@type table<string, string>
---SoundAlert announcement names, exposed under ss.sounds.alerts (for example ss.sounds.alerts.Warning).
ss.sounds.alerts = {}

-- ── Root-level functions on ss table ─────────────────────────────────

---Play a sound effect (game sound or remote URL).
---@param source string Sound name from ss.sounds, or a remote wav/mp3 URL
---@param opts? table Optional: {volume=number, loop=boolean, id=string}
---@return string id Sound element ID for later stop_sound()
function ss.play_sound(source, opts) end

---Stop a playing sound by its ID.
---@param id string Sound element ID returned by play_sound()
function ss.stop_sound(id) end

---Stop all playing sounds on this device.
function ss.stop_all_sounds() end

---Exit interface mode (release keyboard capture back to game).
function ss.exit_interface_mode() end

---Check if interface mode is currently active.
---@return boolean
function ss.is_interface_mode() end

-- ── ss.ui layout helpers ─────────────────────────────────────────────

---Compute grid layout rects for child elements.
---@param opts table {rect={x,y,w,h}, cols=integer, gap?=number, row_height?=number}
---@param children SsElementDef[] Array of child element definitions
---@return SsElementDef[] children Children with computed rect fields
function ss.ui.grid(opts, children) end

---Compute flex layout rects for child elements.
---@param opts table {rect={x,y,w,h}, direction?=string, gap?=number, align?=string, justify?=string}
---@param children SsElementDef[] Array of child element definitions (use flex=N for proportional sizing)
---@return SsElementDef[] children Children with computed rect fields
function ss.ui.flex(opts, children) end

---Generate a color gradient as an array of hex color strings.
---@param from string Start color hex (e.g. "#FF0000")
---@param to string End color hex (e.g. "#00FF00")
---@param steps integer Number of gradient steps
---@return string[] colors Array of interpolated hex color strings
function ss.ui.gradient(from, to, steps) end
