extends Node3D
## Care Packages on the client (`shooter_sim::care_package`), added by `Net`
## when joining a server. For each crate called in:
##
## - the cargo aircraft crosses the map 40 m over the spot along its heading
##   and lets the crate go above it;
## - the crate falls and lands, its amber beacon lit, and shows on the
##   minimap (`minimap.gd`);
## - standing at it, "Hold [F] for UAV" under the crosshair, and a bar while
##   we capture it. The caller takes it in half a second, anyone else in
##   three.
##
## The aircraft drones over on a real turboprop's pass, loudest as it
## crosses the spot, and the crate slams down when it lands
## (`streak_audio.gd`).
##
## The server decides every capture; this draws `NetClient.crates()`.
## Both models are built in Blender (tools/gear_models.py `care_package`,
## `cargo_aircraft`).

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const CRATE_MODEL := preload("res://assets/models/care_package/care_package.gltf")
const AIRCRAFT_MODEL := preload("res://assets/models/cargo_aircraft/cargo_aircraft.gltf")
const StreakAudio := preload("res://scripts/net/streak_audio.gd")
## Seconds into `cargo_flyby` its aircraft is closest.
const FLYBY_CLOSEST := 6.0
## `care_package::FLY_HEIGHT`, `FLY_SPEED`, and `CAPTURE_RADIUS`.
const FLY_HEIGHT := 40.0
const FLY_SPEED := 45.0
const CAPTURE_RADIUS := 1.8
## Seconds either side of the drop the aircraft is drawn.
const FLY_SHOWN := 6.0

var client: NetClient
var player: FpsPlayer

## Crate id -> {"crate": Node3D, "aircraft": Node3D}.
var _drawn := {}
var _hud: Control
var _prompt := {}


func _ready() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 4
	add_child(layer)
	_hud = Control.new()
	_hud.name = "CarePackagePrompt"
	_hud.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_hud.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_hud.draw.connect(_draw_prompt)
	layer.add_child(_hud)


func _process(_delta: float) -> void:
	if client == null or not is_instance_valid(client):
		return
	var crates := client.crates()
	var seen := {}
	_prompt = {}
	for c: Dictionary in crates:
		var id := int(c.id)
		seen[id] = true
		if not _drawn.has(id):
			_drawn[id] = _spawn()
		_place(_drawn[id], c)
		_check_prompt(c)
	for id: int in _drawn.keys():
		if not seen.has(id):
			for key: String in ["crate", "aircraft"]:
				(_drawn[id][key] as Node3D).queue_free()
			_drawn.erase(id)
	_hud.visible = not client.in_killcam()
	_hud.queue_redraw()


func _spawn() -> Dictionary:
	var crate := CRATE_MODEL.instantiate() as Node3D
	crate.name = "CarePackage"
	add_child(crate)
	var beacon := OmniLight3D.new()
	beacon.name = "Beacon"
	beacon.light_color = Color(1.0, 0.62, 0.15)
	beacon.light_energy = 1.4
	beacon.omni_range = 3.5
	beacon.position = Vector3(0, 1.15, 0)
	beacon.shadow_enabled = false
	crate.add_child(beacon)
	var aircraft := AIRCRAFT_MODEL.instantiate() as Node3D
	aircraft.name = "CargoAircraft"
	add_child(aircraft)
	var flyby := StreakAudio.player(aircraft, "cargo_flyby", 35.0, 0.0, 500.0)
	return {"crate": crate, "aircraft": aircraft, "flyby": flyby, "falling": false}


## Places the aircraft along its pass and the crate on its fall.
func _place(nodes: Dictionary, c: Dictionary) -> void:
	var at: Vector3 = c.position
	var heading := float(c.heading)
	var forward := Vector3(-sin(heading), 0.0, -cos(heading))
	var over := at + Vector3.UP * FLY_HEIGHT
	var since_drop := -float(c.drop_seconds)
	var land := float(c.land_seconds)
	var aircraft: Node3D = nodes.aircraft
	aircraft.visible = absf(since_drop) < FLY_SHOWN
	aircraft.position = over + forward * FLY_SPEED * since_drop
	aircraft.rotation = Vector3(0.0, heading, 0.0)
	# The pass is timed so it is loudest over the spot; a late joiner who
	# missed the start hears the rest of it.
	var into := since_drop + FLYBY_CLOSEST
	var flyby: AudioStreamPlayer3D = nodes.flyby
	if not flyby.playing and not bool(nodes.get("flown", false)) and into >= 0.0:
		nodes.flown = true
		if flyby.stream != null and into < flyby.stream.get_length():
			flyby.play(into)
	var crate: Node3D = nodes.crate
	crate.visible = since_drop >= 0.0
	if land > 0.0:
		# Falling: from the aircraft's height to the ground, speeding up.
		var fall := since_drop + land
		var t := clampf(since_drop / maxf(fall, 0.01), 0.0, 1.0)
		crate.position = over.lerp(at, t * t) + forward * FLY_SPEED * 0.15 * (1.0 - t)
		crate.rotation = Vector3(0.0, heading + (1.0 - t) * 0.6, 0.0)
		nodes.falling = true
	else:
		crate.position = at
		crate.rotation = Vector3(0.0, heading, 0.0)
		# Landed this frame (not one already down when we joined).
		if bool(nodes.falling):
			nodes.falling = false
			StreakAudio.play_at(self, at, "crate_land", 10.0, 0.0, 150.0)
	(crate.get_node(^"Beacon") as OmniLight3D).visible = land <= 0.0


func _check_prompt(c: Dictionary) -> void:
	if player == null or not player.is_alive() or float(c.land_seconds) > 0.0:
		return
	var feet := player.global_position
	var at: Vector3 = c.position
	var flat := Vector2(feet.x - at.x, feet.z - at.z).length()
	if flat > CAPTURE_RADIUS or absf(feet.y - at.y) > 2.0:
		return
	_prompt = c


func _draw_prompt() -> void:
	if _prompt.is_empty():
		return
	var centre := Vector2(_hud.size.x / 2.0, _hud.size.y * 0.62)
	var me := client.player_id()
	var capturing := int(_prompt.capture_by) == me
	var name := String(_prompt.contents_name)
	if capturing:
		var bar := Rect2(centre.x - 110.0, centre.y + 8.0, 220.0, 5.0)
		Paint.text_centred(
			_hud, Paint.LABEL, "CAPTURING %s" % name.to_upper(), centre, 15, UiTheme.TEXT, 2.0
		)
		_hud.draw_rect(bar, Paint.TRACK)
		var fill := float(_prompt.capture_progress)
		_hud.draw_rect(Rect2(bar.position, Vector2(bar.size.x * fill, bar.size.y)), UiTheme.ACCENT)
	elif int(_prompt.capture_by) < 0:
		var key := Settings.prompt_key(&"interact")
		var text := "[%s] HOLD FOR %s" % [key, name.to_upper()]
		var prompt := Paint.split_key(text, UiTheme.TEXT)
		Paint.prompt(_hud, prompt, centre.x, centre.y, 16)
