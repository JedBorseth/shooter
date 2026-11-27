extends Control
## Scorestreaks on the HUD (`shooter_sim::streak`), added by `Net` to the
## match HUD, laid out as Black Ops 3 lays out its own:
##
## - the three chosen streaks in a column of small slots, bottom right above
##   the health ring, the cheapest lowest, each with its icon (dim until
##   earned), and above them a slot for one held from a Care Package (a thin
##   cross when empty). Beside the column a thin rail fills with this life's
##   score toward the next streak, with that streak's cost in a tag at its top.
##   An earned slot lights with a yellow border, and a pointer beside it marks
##   the one the use key calls in;
## - "UAV READY" on a yellow plate at the bottom centre when one is earned,
##   with the key to call it in;
## - short notices under the minimap when a UAV or Counter-UAV goes up, ours
##   or the enemy's ("UAV online", "Enemy UAV overhead", "Enemy Counter-UAV"),
##   and when one is shot down ("Enemy UAV destroyed", "UAV shot down").
##
## The use key (4, or D-pad right) calls in the selected streak; the next
## key (5, or D-pad left) selects another held one. The server decides
## everything; this only draws what `NetClient.streak_info` says.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const CALL_SOUND := preload("res://assets/audio/ui/hardpoint_call.ogg")

## Slot size and the gap between slots.
const SLOT := Vector2(66.0, 44.0)
const SLOT_GAP := 4.0
## Pixels from the screen's right and bottom edges to the column's bottom
## right corner (above the health ring).
const RIGHT := 52.0
const BOTTOM := 178.0
## The ready plate's centre above the screen's bottom edge, its size, and
## how long it holds.
const READY_UP := 150.0
const READY_SIZE := Vector2(220.0, 36.0)
const READY_SECONDS := 3.0
## Notices: their first baseline from the screen's top (under the minimap and
## the mode's rows), and how long they stay.
const NOTICE_TOP := 326.0
const NOTICE_SECONDS := 4.0

var client: NetClient
var player: FpsPlayer
## Calls made, in order (tests).
var calls: PackedStringArray = []

var _info := {}
var _names := {}
## The ready plate: the streak's name, and seconds since it was earned.
var _ready_name := ""
var _ready_age := -1.0
## Notices under the minimap: {"text", "colour", "age"}, newest last.
var _notices: Array[Dictionary] = []
var _sound: AudioStreamPlayer


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	for item: Dictionary in NetClient.streak_items():
		_names[String(item.key)] = String(item.name)
	_sound = AudioStreamPlayer.new()
	_sound.bus = &"UI"
	_sound.stream = CALL_SOUND
	_sound.volume_db = -8.0
	add_child(_sound)
	if client == null:
		return
	client.streaks_changed.connect(_on_streaks)
	client.sky_changed.connect(_on_sky)
	client.aircraft_downed.connect(_on_downed)


func _unhandled_input(event: InputEvent) -> void:
	if client == null or not visible or player == null or not player.is_alive():
		return
	if event.is_action_pressed(&"streak_use"):
		get_viewport().set_input_as_handled()
		client.use_streak()
	elif event.is_action_pressed(&"streak_next"):
		get_viewport().set_input_as_handled()
		client.cycle_streak()


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		visible = false
		return
	var mode := String(client.match_info().get("mode", ""))
	# The results screen clears the play HUD, as in Call of Duty.
	var ended := String(client.match_info().get("phase", "")) == "ended"
	# A custom game can turn scorestreaks off.
	var off := bool(client.match_info().get("streaks_off", false))
	visible = (mode != "gungame" and not off and not client.in_killcam() and not ended)
	_info = client.streak_info()
	# Under the HUD size setting the piece is scaled: keep it screen-sized.
	size = get_viewport_rect().size / scale
	if _ready_age >= 0.0:
		_ready_age += delta
		if _ready_age > READY_SECONDS + 0.3:
			_ready_age = -1.0
	for notice in _notices:
		notice.age += delta
	_notices = _notices.filter(func(notice: Dictionary) -> bool: return notice.age < NOTICE_SECONDS)
	queue_redraw()


func _on_streaks(earned: String) -> void:
	if earned == "":
		return
	calls.append("%s READY" % _name(earned).to_upper())
	_ready_name = _name(earned)
	_ready_age = 0.0
	if is_inside_tree():
		_sound.play()


func _on_sky(called: String, ours: bool) -> void:
	match [called, ours]:
		["uav", true]:
			_say("UAV ONLINE", TeamColours.ally)
		["uav", false]:
			_say("ENEMY UAV OVERHEAD", TeamColours.enemy)
		["counter_uav", true]:
			_say("COUNTER-UAV ONLINE", TeamColours.ally)
		["counter_uav", false]:
			_say("ENEMY COUNTER-UAV", TeamColours.enemy)


## Black Ops 2's announcer calls a UAV shot down for both sides; the
## medal for the one who did it comes with the score.
func _on_downed(streak: String, ours: bool, _by_me: bool, _at: Vector3) -> void:
	var name := _name(streak).to_upper()
	if ours:
		_say("%s SHOT DOWN" % name, TeamColours.enemy)
	else:
		_say("ENEMY %s DESTROYED" % name, TeamColours.ally)


## A notice under the minimap, in the side's colour.
func _say(text: String, colour: Color) -> void:
	calls.append(text)
	_notices.append({"text": text, "colour": colour, "age": 0.0})
	if _notices.size() > 3:
		_notices.pop_front()
	if is_inside_tree():
		_sound.play()


func _name(key: String) -> String:
	return String(_names.get(key, key))


func _draw() -> void:
	if _info.is_empty():
		return
	draw_set_transform_matrix(Shapes.tilt(size.x - RIGHT, -Shapes.TILT))
	_draw_column()
	draw_set_transform_matrix(Shapes.tilt(40.0, Shapes.TILT))
	_draw_notices()
	draw_set_transform_matrix(Transform2D.IDENTITY)
	if _ready_age >= 0.0:
		_draw_ready()


## The column: the chosen streaks from the bottom up, cheapest lowest, and
## the held slot on top; the rail beside it and the next streak's cost.
func _draw_column() -> void:
	var loadout: PackedStringArray = _info.get("loadout", PackedStringArray())
	var held: PackedStringArray = _info.get("held", PackedStringArray())
	var selected := int(_info.get("selected", -1))
	var selected_key := held[selected] if selected >= 0 and selected < held.size() else ""
	var progress := float(_info.get("progress", 0))
	var right := size.x - RIGHT
	var bottom := size.y - BOTTOM
	# Always three streak slots and the held slot, so the column never grows
	# mid-match; a slot with no streak chosen shows the empty cross.
	var count := maxi(loadout.size(), 3)
	var slots: Array[Rect2] = []
	for i in count + 1:
		var y := bottom - (i + 1) * SLOT.y - i * SLOT_GAP
		slots.append(Rect2(right - SLOT.x, y, SLOT.x, SLOT.y))
	for i in count:
		if i >= loadout.size():
			_draw_empty(slots[i])
			continue
		var key := loadout[i]
		_draw_slot(slots[i], key, held.count(key), key == selected_key)
	# The held slot: a streak from a Care Package, or empty.
	var extra := ""
	for key in held:
		if not key in loadout:
			extra = key
			break
	var top := slots[count]
	if extra != "":
		_draw_slot(top, extra, held.count(extra), extra == selected_key)
	else:
		_draw_empty(top)
	if not loadout.is_empty():
		_draw_rail(slots, loadout, progress)


## An empty slot: dark, a thin border, and a thin cross.
func _draw_empty(rect: Rect2) -> void:
	Shapes.smooth_rect(self, rect, Color(0, 0, 0, 0.4))
	Shapes.smooth_rect(self, rect, Color(1, 1, 1, 0.26), false, 1.0)
	Shapes.empty_cross(self, rect.grow(-1.0), Color(1, 1, 1, 0.26))


## One slot: dark with a thin border and the streak's icon, dim until earned;
## earned, a yellow border, a warm fill, and a pointer on its right edge.
func _draw_slot(rect: Rect2, key: String, count: int, selected: bool) -> void:
	var earned := count > 0
	if earned:
		Shapes.smooth_rect(self, rect, Color(0.5, 0.41, 0.06, 0.62))
		Shapes.smooth_rect(self, rect, UiTheme.READY, false, 2.0)
		if selected:
			Shapes.pointer(
				self,
				Vector2(rect.end.x + 7.0, rect.get_center().y),
				8.0,
				Vector2.RIGHT,
				UiTheme.READY
			)
	else:
		Shapes.smooth_rect(self, rect, Color(0, 0, 0, 0.5))
		Shapes.smooth_rect(self, rect, Color(1, 1, 1, 0.32), false, 1.0)
	var colour := UiTheme.TEXT if earned else Color(UiTheme.TEXT, 0.4)
	_draw_icon(key, rect.get_center(), colour)
	if count > 1:
		Paint.text_right(
			self, Paint.LABEL, "×%d" % count, rect.end - Vector2(4.0, 3.0), 13, UiTheme.TEXT
		)


## The rail left of the column, filling with this life's score from the
## bottom slot's foot up through each slot's middle as its cost is reached,
## and a tag at its top with the next streak's cost.
func _draw_rail(slots: Array[Rect2], loadout: PackedStringArray, progress: float) -> void:
	var x := slots[0].position.x - 8.0
	var foot := slots[0].end.y
	var head := slots[loadout.size() - 1].get_center().y
	draw_line(Vector2(x, head), Vector2(x, foot), Color(1, 1, 1, 0.2), 2.0, true)
	var fill_y := foot
	var below := 0.0
	var next_cost := -1
	var from_y := foot
	for i in loadout.size():
		var cost := float(_cost(loadout[i]))
		var to_y := slots[i].get_center().y
		if progress >= cost:
			fill_y = to_y
		else:
			var t := clampf((progress - below) / maxf(cost - below, 1.0), 0.0, 1.0)
			fill_y = lerpf(from_y, to_y, t)
			next_cost = int(cost)
			break
		below = cost
		from_y = to_y
	if fill_y < foot:
		draw_line(Vector2(x, fill_y), Vector2(x, foot), Color(UiTheme.TEXT, 0.92), 2.0, true)
	if next_cost < 0:
		return
	var text := str(next_cost)
	var tag_w := Paint.text_width(Paint.LABEL, text, 14) + 10.0
	var tag := Rect2(x - 6.0 - tag_w, head - 9.0, tag_w, 18.0)
	Shapes.smooth_rect(self, tag, Color(0, 0, 0, 0.62))
	Shapes.smooth_rect(self, tag, Color(1, 1, 1, 0.24), false, 1.0)
	Paint.text_centred(
		self, Paint.LABEL, text, Vector2(tag.get_center().x, tag.end.y - 4.0), 14, UiTheme.TEXT
	)


## The yellow ready plate at the bottom centre: rises and fades in, holds,
## then fades.
func _draw_ready() -> void:
	var rise := 1.0 - clampf(_ready_age / 0.15, 0.0, 1.0)
	var alpha := clampf(_ready_age / 0.15, 0.0, 1.0)
	alpha *= clampf((READY_SECONDS + 0.3 - _ready_age) / 0.3, 0.0, 1.0)
	var centre := Vector2(size.x / 2.0, size.y - READY_UP + rise * 12.0)
	var title := "%s READY" % _ready_name.to_upper()
	var width := maxf(READY_SIZE.x, Paint.text_width(Paint.DISPLAY, title, 21, 2.0) + 80.0)
	var plate := Rect2(centre - Vector2(width, READY_SIZE.y) / 2.0, Vector2(width, READY_SIZE.y))
	draw_rect(plate.grow(1.0), Color(0, 0, 0, 0.3 * alpha))
	draw_rect(plate, Color(UiTheme.READY, 0.96 * alpha))
	Paint.text_centred(
		self,
		Paint.DISPLAY,
		title,
		Vector2(centre.x, plate.end.y - 10.0),
		21,
		Color(UiTheme.READY_INK, alpha),
		2.0,
		false
	)
	var key := Settings.prompt_key(&"streak_use")
	Paint.prompt(
		self,
		{"text": "Press [%s]" % key, "colour": UiTheme.TEXT},
		centre.x,
		plate.end.y + 26.0,
		17,
		alpha,
		false
	)


## Notices under the minimap, newest at the bottom: a short line in the
## side's colour on a faint plate with a tab in that colour.
func _draw_notices() -> void:
	var y := NOTICE_TOP
	for notice in _notices:
		var age: float = notice.age
		var alpha := clampf(age / 0.15, 0.0, 1.0) * clampf((NOTICE_SECONDS - age) / 0.4, 0.0, 1.0)
		var colour: Color = notice.colour
		var text: String = notice.text
		var width := Paint.text_width(Paint.LABEL, text, 16, 1.2) + 22.0
		var plate := Rect2(40.0, y - 18.0, width, 24.0)
		Paint.fade(
			self, Rect2(plate.position, plate.size + Vector2(30, 0)), Color(0, 0, 0, 0.45 * alpha)
		)
		draw_rect(Rect2(plate.position, Vector2(3.0, plate.size.y)), Color(colour, alpha))
		Paint.text(
			self,
			Paint.LABEL,
			text,
			Vector2(52.0, y),
			16,
			Color(colour.lerp(Color.WHITE, 0.25), alpha),
			1.2
		)
		y += 28.0


## Our own small icons for each streak, drawn in code: a drone seen from
## above for the UAV, the same drone sending jamming waves for the
## Counter-UAV, and a strapped crate for the Care Package.
func _draw_icon(key: String, at: Vector2, colour: Color) -> void:
	match key:
		"uav":
			# A drone from above: fuselage, long straight wings, a V tail.
			draw_colored_polygon(
				PackedVector2Array(
					[
						at + Vector2(0, -12),
						at + Vector2(2.2, -8),
						at + Vector2(1.6, 9),
						at + Vector2(-1.6, 9),
						at + Vector2(-2.2, -8),
					]
				),
				colour
			)
			draw_colored_polygon(
				PackedVector2Array(
					[
						at + Vector2(-18, -3),
						at + Vector2(18, -3),
						at + Vector2(17, 0),
						at + Vector2(-17, 0),
					]
				),
				colour
			)
			draw_line(at + Vector2(0, 7), at + Vector2(6, 11), colour, 2.0, true)
			draw_line(at + Vector2(0, 7), at + Vector2(-6, 11), colour, 2.0, true)
		"counter_uav":
			# A jamming mast: a lattice tower with waves going out both ways.
			var top := at + Vector2(0, -10)
			draw_line(top, at + Vector2(-6, 13), colour, 2.0, true)
			draw_line(top, at + Vector2(6, 13), colour, 2.0, true)
			draw_line(at + Vector2(-4, 5), at + Vector2(4, 5), colour, 1.5, true)
			draw_line(at + Vector2(-2.2, -2), at + Vector2(2.2, -2), colour, 1.5, true)
			draw_circle(top, 2.5, colour, true, -1.0, true)
			for i in 3:
				var r := 6.0 + i * 4.5
				draw_arc(top, r, -0.7, 0.7, 10, colour, 1.6, true)
				draw_arc(top, r, PI - 0.7, PI + 0.7, 10, colour, 1.6, true)
		"care_package":
			# A strapped crate hanging under a small canopy.
			var box := Rect2(at + Vector2(-9, 0), Vector2(18, 12))
			draw_rect(box, colour)
			var strap := Color(0, 0, 0, 0.55 * colour.a)
			draw_rect(Rect2(box.position + Vector2(0, 5), Vector2(box.size.x, 2)), strap)
			draw_rect(Rect2(box.position + Vector2(8, 0), Vector2(2, box.size.y)), strap)
			var canopy := at + Vector2(0, -6)
			draw_arc(canopy, 11.0, PI + 0.25, TAU - 0.25, 18, colour, 3.0, true)
			for side: float in [-1.0, 1.0]:
				draw_line(
					canopy + Vector2(side * 10.5, -1.5),
					box.position + Vector2(9 + side * 9, 0),
					colour,
					1.2,
					true
				)
			draw_line(canopy + Vector2(0, -11), box.position + Vector2(9, 0), colour, 1.2, true)
		_:
			Paint.text_centred(
				self, Paint.LABEL, _name(key).to_upper(), at + Vector2(0, 5), 13, colour
			)


func _cost(key: String) -> int:
	for item: Dictionary in NetClient.streak_items():
		if String(item.key) == key:
			return int(item.cost)
	return 0
