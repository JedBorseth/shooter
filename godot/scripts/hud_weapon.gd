extends Control
## The weapon cluster, bottom right, as Black Ops 3 lays out its own (left of
## the health ring, `hud_vitals.gd`): the ammo on a dark plate with a light
## bracket edge, the magazine count big and the reserve smaller in two cells,
## the weapon's name under it, and above it two small bordered slots for the
## tactical and lethal grenades with their counts (a thin cross when empty).
## The cluster is turned a little toward the screen's centre.
##
## Low on ammo, the magazine count turns red; with no reserve left, the
## reserve does too; while reloading both dim.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")

## Room kept on the right for the health ring.
const RING_SPACE := 146.0
const MAG_W := 88.0
const RESERVE_W := 78.0
const PLATE_H := 56.0
const SLOT := 42.0

var player: FpsPlayer

## Brightens briefly when the weapon changes, so the swap reads.
var _flash := 0.0
var _key := ""


func _process(delta: float) -> void:
	if player == null:
		return
	var key := player.weapon_key()
	if key != _key:
		_key = key
		_flash = 1.0
	_flash = maxf(_flash - delta * 2.5, 0.0)
	queue_redraw()


func _draw() -> void:
	if player == null:
		return
	var right := size.x - RING_SPACE
	var bottom := size.y
	# A soft shade from the screen's corner, so the name holds over a bright
	# scene.
	Paint.corner_shade(
		self, Rect2(size.x - 560.0, bottom - 190.0, 600.0, 224.0), Vector2(1, 1), 0.4
	)
	draw_set_transform_matrix(Shapes.tilt(size.x, -Shapes.TILT))

	var plate := Rect2(right - MAG_W - RESERVE_W, bottom - 112.0, MAG_W + RESERVE_W, PLATE_H)
	_draw_plate(plate)
	var magazine := player.magazine()
	var reserve := player.reserve()
	var low := magazine <= player.low_ammo_threshold()
	var reloading := player.is_reloading()
	var mag_colour := UiTheme.HARM if low else UiTheme.TEXT
	var reserve_colour := UiTheme.HARM if reserve == 0 else Color(UiTheme.TEXT, 0.82)
	if reloading:
		mag_colour = Color(UiTheme.TEXT, 0.45)
		reserve_colour = Color(UiTheme.TEXT, 0.4)
	var baseline := plate.end.y - 12.0
	Paint.text_centred(
		self,
		Paint.DISPLAY,
		str(magazine),
		Vector2(plate.position.x + MAG_W / 2.0 + 3.0, baseline + 2.0),
		44,
		mag_colour
	)
	Paint.text_centred(
		self,
		Paint.DISPLAY,
		str(reserve),
		Vector2(plate.end.x - RESERVE_W / 2.0, baseline - 2.0),
		27,
		reserve_colour
	)

	_draw_name(Vector2(plate.end.x, plate.end.y + 24.0))
	_draw_slots(Vector2(plate.end.x, plate.position.y - 6.0))
	draw_set_transform_matrix(Transform2D.IDENTITY)


## The ammo plate: dark, with its top-left corner cut, a light edge along the
## top and the cut, and a thin divider between the two counts.
func _draw_plate(rect: Rect2) -> void:
	var cut := 12.0
	var points := PackedVector2Array(
		[
			rect.position + Vector2(cut, 0),
			Vector2(rect.end.x, rect.position.y),
			rect.end,
			Vector2(rect.position.x, rect.end.y),
			rect.position + Vector2(0, cut),
		]
	)
	draw_colored_polygon(points, Color(0, 0, 0, 0.55))
	# A bracket: the cut and the top bright, the right side fainter.
	var edge := PackedVector2Array(
		[Vector2(rect.position.x, rect.end.y - 10.0), points[4], points[0], points[1]]
	)
	draw_polyline(edge, Color(1, 1, 1, 0.6), 1.0, true)
	draw_line(points[1], points[2] - Vector2(0, 10.0), Color(1, 1, 1, 0.35), 1.0, true)
	var divider := rect.position.x + MAG_W
	draw_line(
		Vector2(divider, rect.position.y + 9.0),
		Vector2(divider, rect.end.y - 9.0),
		Color(1, 1, 1, 0.35),
		1.0,
		true
	)


## The weapon's name under the plate, mixed case as Black Ops 3 writes it,
## with its attachments after it in quieter type. It brightens on a swap.
func _draw_name(right: Vector2) -> void:
	var parts := Paint.split_weapon_name(_key, player.weapon_name())
	var name := parts[0]
	var attachment := parts[1]
	var name_size := 19
	var tag_size := 16
	var tag := ("  " + attachment) if attachment != "" else ""
	var name_w := Paint.text_width(Paint.LABEL, name, name_size)
	var tag_w := Paint.text_width(Paint.LABEL_LIGHT, tag, tag_size)
	var x := right.x - name_w - tag_w
	var colour := UiTheme.TEXT.lerp(Color.WHITE, _flash)
	Paint.text(self, Paint.LABEL, name, Vector2(x, right.y), name_size, colour)
	if tag != "":
		Paint.text(
			self,
			Paint.LABEL_LIGHT,
			tag,
			Vector2(x + name_w, right.y),
			tag_size,
			Color(UiTheme.TEXT, 0.75)
		)


## The tactical and lethal slots above the plate's right end: small dark
## squares with a thin border, the grenade and its count, or a cross when
## none are left.
func _draw_slots(bottom_right: Vector2) -> void:
	var lethal := Rect2(bottom_right - Vector2(SLOT, SLOT), Vector2(SLOT, SLOT))
	var tactical := Rect2(lethal.position - Vector2(SLOT + 6.0, 0), lethal.size)
	var frags := player.lethals()
	var tacticals := player.tacticals()
	for slot: Array in [[lethal, frags, false], [tactical, tacticals, true]]:
		var rect: Rect2 = slot[0]
		var count: int = slot[1]
		Shapes.smooth_rect(self, rect, Color(0, 0, 0, 0.52))
		Shapes.smooth_rect(self, rect, Color(1, 1, 1, 0.36), false, 1.0)
		if count <= 0:
			Shapes.empty_cross(self, rect.grow(-5.0), Color(1, 1, 1, 0.32))
			continue
		var centre := rect.get_center() + Vector2(-3.0, 1.0)
		if slot[2]:
			Paint.canister(self, centre, 26.0, UiTheme.TEXT, player.tactical_key() == "smoke")
		else:
			Paint.grenade(self, centre, 26.0, UiTheme.TEXT)
		Paint.text_right(
			self, Paint.LABEL, str(count), rect.end - Vector2(3.0, 3.0), 13, UiTheme.TEXT
		)
