extends SceneTree
## Captures every weapon for reviewing the weapon models: first person at
## the hip, aimed, mid-reload, and twice through an inspect, then other
## players holding them and the weapon racks. Needs a GPU and a display (or
## Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_weapons.gd
## Optional: MAP (main or zombies_arena, default main), WEAPONS (for example
## `m4,ak47+reddot`), and PARTS (`first,third,racks`, default all three).

const WEAPONS := [
	"m4",
	"ak47",
	"kr12",
	"pdw9",
	"cq10",
	"m870",
	"sg12",
	"dm7",
	"m40",
	"m1911",
	"bp9",
	"rl4",
	"lsw6",
	"br3",
	"m4+reddot",
	"m4+acog",
	"m4+suppressor",
	"m4+grip",
	"m4+extmag",
]
const MAPS := {
	"main": "res://scenes/main.tscn",
	"zombies_arena": "res://scenes/zombies_arena.tscn",
}

var _out := OS.get_environment("CAPTURE_DIR")
var _map := OS.get_environment("MAP")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	if _map.is_empty():
		_map = "main"
	DirAccess.make_dir_recursive_absolute(_out)
	var level: Node = (load(MAPS[_map]) as PackedScene).instantiate()
	root.add_child(level)
	# No sentry fire or zombie waves while capturing.
	for node: Node in level.find_children("*", "", true, false):
		if node.name == "Sentry" or node.get_class() == "ZombieDirector":
			node.process_mode = Node.PROCESS_MODE_DISABLED
	var player := level.get_node("Player") as FpsPlayer
	player.set_look(0.0, -0.04)
	await _sixtieths(60)
	var weapons: Array = WEAPONS
	var only := OS.get_environment("WEAPONS")
	if not only.is_empty():
		weapons = Array(only.split(","))
	var parts := OS.get_environment("PARTS")
	if parts.is_empty():
		parts = "first,third,racks"
	if "first" in parts:
		for weapon: String in weapons:
			await _first_person(player, weapon)
	if "third" in parts:
		await _third_person(level, player, weapons)
	if "racks" in parts and _map == "main":
		await _racks(level, player)
	quit()


func _first_person(player: FpsPlayer, weapon: String) -> void:
	var tag := "%s_%s" % [_map, weapon.replace("+", "_")]
	# Sidearms go in the second slot (the M1911 is already there).
	var path := "res://data/weapons/%s.ron" % weapon.get_slice("+", 0)
	var slot := (
		"weapon_2"
		if FileAccess.get_file_as_string(path).contains("slot: Secondary")
		else "weapon_1"
	)
	if weapon != "m1911" and not player.give_weapon(weapon):
		push_error("capture_weapons: no weapon `%s`" % weapon)
		return
	Input.action_press(slot)
	await _sixtieths(2)
	Input.action_release(slot)
	player.refill_all_ammo()
	await _sixtieths(80)
	if player.weapon_key() != weapon:
		push_error("capture_weapons: holding `%s`, not `%s`" % [player.weapon_key(), weapon])
		return
	await _shot("%s_1hip" % tag)
	Input.action_press("aim")
	await _sixtieths(45)
	await _shot("%s_2ads" % tag)
	Input.action_release("aim")
	await _sixtieths(30)
	# Mid-reload: the magazine out, or a round going in. Fire a few rounds
	# first, so there is something to reload.
	Input.action_press("fire")
	# Mid-burst, with a case in the air.
	await _sixtieths(4)
	await _shot("%s_0fire" % tag)
	await _sixtieths(16)
	Input.action_release("fire")
	await _sixtieths(40)
	await _tap("reload")
	await _sixtieths(75)
	await _shot("%s_3reload" % tag)
	for i in 300:
		if not player.is_reloading():
			break
		await physics_frame
	await _sixtieths(20)
	# Inspect: the left side turned up, then the top rolled over.
	await _tap("inspect")
	await _sixtieths(46)
	await _shot("%s_4inspect" % tag)
	await _sixtieths(56)
	await _shot("%s_5inspect" % tag)
	await _sixtieths(60)


## Other players holding each weapon, close up from a free camera.
func _third_person(level: Node, player: FpsPlayer, weapons: Array) -> void:
	var scene := load("res://scenes/characters/soldier.tscn") as PackedScene
	var viewmodel := player.get_node("Camera/Viewmodel") as Node3D
	viewmodel.visible = false
	var layers := root.find_children("*", "CanvasLayer", true, false)
	for layer: CanvasLayer in layers:
		layer.visible = false
	var hero := scene.instantiate() as SoldierRig
	level.add_child(hero)
	var base := player.global_position + Vector3(0, 0, -3.0)
	hero.global_position = base
	var camera := Camera3D.new()
	camera.fov = 35
	level.add_child(camera)
	camera.look_at_from_position(base + Vector3(1.3, 1.5, 2.2), base + Vector3(0, 1.3, 0))
	camera.make_current()
	var far := Camera3D.new()
	far.fov = 50
	level.add_child(far)
	for weapon: String in weapons:
		for frame in 45:
			hero.drive({"weapon": weapon, "yaw": -0.5, "pitch": 0.0}, 1.0 / 60.0)
			await process_frame
		camera.make_current()
		await _shot("%s_%s_6third" % [_map, weapon.replace("+", "_")])
		far.look_at_from_position(base + Vector3(3.5, 1.7, 7.0), base + Vector3(0, 1.1, 0))
		far.make_current()
		await _shot("%s_%s_7third_far" % [_map, weapon.replace("+", "_")])
	hero.queue_free()
	camera.queue_free()
	far.queue_free()
	for layer: CanvasLayer in layers:
		layer.visible = true
	viewmodel.visible = true
	(player.get_node("Camera") as Camera3D).make_current()
	await _sixtieths(5)


## The armory's weapon racks, near and far.
func _racks(level: Node, player: FpsPlayer) -> void:
	var armory := level.get_node_or_null("Armory") as Node3D
	if armory == null:
		return
	var layers := root.find_children("*", "CanvasLayer", true, false)
	for layer: CanvasLayer in layers:
		layer.visible = false
	(player.get_node("Camera/Viewmodel") as Node3D).visible = false
	var camera := Camera3D.new()
	camera.fov = 45
	level.add_child(camera)
	var racks := armory.get_children()
	for i in racks.size():
		var rack := racks[i] as Node3D
		var at := rack.global_position + Vector3(0, 1.1, 0)
		camera.look_at_from_position(at + rack.global_basis.z * 1.3 + Vector3(0, 0.6, 0), at)
		camera.make_current()
		await _sixtieths(3)
		await _shot("%s_rack_%s" % [_map, rack.name])
	var first := (racks[0] as Node3D).global_position
	var last := (racks[racks.size() - 1] as Node3D).global_position
	var mid := (first + last) * 0.5
	camera.look_at_from_position(
		mid + (racks[0] as Node3D).global_basis.z * 5.0 + Vector3(0, 2.0, 0),
		mid + Vector3(0, 1.0, 0)
	)
	await _sixtieths(3)
	await _shot("%s_racks_all" % _map)
	camera.queue_free()


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _tap(action: String) -> void:
	# Held across ticks: a press and release inside one tick is only caught through input events.
	Input.action_press(action)
	await _sixtieths(2)
	Input.action_release(action)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
