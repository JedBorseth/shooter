extends Control
## Engineer on the HUD (`shooter_sim::perk::Perk::Engineer`), added by `Net`
## to the match HUD: with the perk, every landed Care Package not ours shows
## through walls as a small red marker, as Black Ops 2's Engineer outlines
## enemy equipment and scorestreaks. Without it, nothing draws.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const ICON := preload("res://assets/ui/perks/engineer.svg")
const SIZE := 9.0

var client: NetClient
var _marks: Array[Dictionary] = []


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE


func _process(_delta: float) -> void:
	_marks.clear()
	if client == null or not is_instance_valid(client) or client.in_killcam():
		queue_redraw()
		return
	if client.has_perk("engineer"):
		var camera := get_viewport().get_camera_3d()
		if camera != null:
			for c: Dictionary in client.crates():
				if not bool(c.ours) and float(c.land_seconds) <= 0.0:
					_mark(camera, (c.position as Vector3) + Vector3.UP * 0.9, "CRATE")
	queue_redraw()


func _mark(camera: Camera3D, at: Vector3, label: String) -> void:
	if camera.is_position_behind(at):
		return
	_marks.append({"at": camera.unproject_position(at), "label": label})


func _draw() -> void:
	var colour := Color(UiTheme.ENEMY, 0.9)
	for m: Dictionary in _marks:
		var p: Vector2 = m.at
		var diamond := PackedVector2Array(
			[
				p + Vector2(0, -SIZE),
				p + Vector2(SIZE, 0),
				p + Vector2(0, SIZE),
				p + Vector2(-SIZE, 0),
				p + Vector2(0, -SIZE)
			]
		)
		draw_polyline(diamond, Color(0, 0, 0, 0.5), 4.0)
		draw_polyline(diamond, colour, 2.0)
		Paint.text_centred(
			self, Paint.LABEL, String(m.label), p + Vector2(0, -SIZE - 6.0), 11, colour, 1.0
		)
