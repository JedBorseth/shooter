extends SceneTree
## Records how the first-person weapon and the view move, frame by frame, for
## reviewing weapon feel: aiming, firing, recoil, sway, movement, reloads,
## switches, and inspects. Writes one CSV row per rendered frame.
##
## Headless trace (no GPU needed):
##   TRACE_DIR=/path/to/out godot --headless --fixed-fps 60 --path godot \
##     --script res://tests/capture_weapon_feel.gd
## Frame strips as well (needs a GPU and a display or Xvfb): set FRAMES_DIR,
## and optionally SCENARIOS (for example `spray_ads,reload_m4`) to run only
## some scenarios. Every frame of each scenario is saved as a PNG, or every
## Nth with FRAME_EVERY=N.

const SCENARIOS := [
	"ads_m4",
	"spray_hip_m4",
	"spray_ads_m4",
	"spray_ads_ak47",
	"spray_ads_pdw9",
	"spray_ads_lsw6",
	"taps_m1911",
	"burst_br3",
	"shot_m40",
	"shot_m870",
	"flick_hip",
	"flick_ads",
	"idle_ads",
	"walk_sprint",
	"strafe",
	"jump",
	"reload_m4",
	"reload_m4_empty",
	"reload_ak47_empty",
	"reload_m40_empty",
	"reload_lsw6",
	"reload_m1911_empty",
	"reload_m870",
	"reload_cq10",
	"reload_cq10_empty",
	"reload_bp9",
	"reload_bp9_empty",
	"reload_kr12",
	"reload_kr12_empty",
	"reload_dm7_empty",
	"reload_sg12",
	"reload_sg12_empty",
	"reload_rl4",
	"reload_ads_m4",
	"ads_acog",
	"spray_ads_acog",
	"ads_ak47_acog",
	"ads_pdw9_acog",
	"reload_fire_m4",
	"switch",
	"inspect",
	"inspect_m1911",
	"inspect_m870",
	"inspect_m40",
	"inspect_lsw6",
	"inspect_cq10",
	"inspect_bp9",
	"first_raise",
	"first_raise_bolt",
	"first_raise_pump",
	"throw",
	"throw_quick",
	"slide",
	"crouch",
	"crawl",
	"crawl_m1911",
	"drink",
	"drink_m1911",
	"revive",
	"defuse",
	"vault",
	"mantle",
	"hit",
]

## Weapon keys a scenario's name may contain.
const WEAPONS := [
	"m4",
	"ak47",
	"br3",
	"m40",
	"m870",
	"lsw6",
	"pdw9",
	"m1911",
	"cq10",
	"bp9",
	"kr12",
	"dm7",
	"sg12",
	"rl4"
]

var _out := OS.get_environment("TRACE_DIR")
var _frames_dir := OS.get_environment("FRAMES_DIR")
var _frame_every := maxi(1, int(OS.get_environment("FRAME_EVERY")))
var _rows := 0
var _player: FpsPlayer
var _camera: Camera3D
var _rig: Node3D
var _file: FileAccess
var _scenario := ""
var _label := ""
var _t := 0.0
var _frame := 0
var _turn := Vector2.ZERO
## Foley events the weapon emitted since the last row.
var _foley: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set TRACE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	_player = main.get_node("Player") as FpsPlayer
	_camera = _player.get_node("Camera") as Camera3D
	_rig = _camera.get_node("Viewmodel") as Node3D
	main.get_node("Sentry").set("enabled", false)
	var hud := main.get_node_or_null("HUD")
	if hud != null:
		hud.set("visible", false)
	_file = FileAccess.open(_out.path_join("trace.csv"), FileAccess.WRITE)
	(
		_file
		. store_line(
			(
				"scenario,label,t,ads,action,magazine,fov,"
				+ "vm_x,vm_y,vm_z,vm_pitch,vm_yaw,vm_roll,"
				+ "cam_pitch,cam_yaw,cam_roll,aim_pitch,aim_yaw,look_pitch,"
				+ "sight_pitch,sight_yaw,speed,hand_x,hand_y,hand_z,hand_eye,mag_u,mag_v,mag_depth,foley"
			)
		)
	)
	# Recorded at the end of each frame, after every node has processed, so
	# the weapon's pose and the player's state are from the same frame.
	process_frame.connect(func() -> void: _record.call_deferred())
	for child in _rig.get_children():
		if child is WeaponView:
			child.connect("foley_event", func(name: StringName) -> void: _foley.append(name))
	var only := OS.get_environment("SCENARIOS")
	var scenarios := SCENARIOS
	if not only.is_empty():
		scenarios = Array(only.split(","))
	for scenario: String in scenarios:
		await _start(scenario)
		await call("_scenario_" + scenario)
		_release_all()
	_scenario = ""
	_file.close()
	print("trace written to ", _out)
	quit()


## Fresh state: back at the spawn, looking along -Z, idle, with `weapon` drawn.
func _start(scenario: String) -> void:
	_scenario = ""
	_release_all()
	_turn = Vector2.ZERO
	_player.refill_all_ammo()
	_player.teleport(Vector3(0, 0.1, 20), 0.0)
	_player.set_look(0.0, 0.0)
	# The weapon is whichever weapon key the scenario name contains.
	var weapon := "m4"
	for part in scenario.split("_"):
		if part in WEAPONS:
			weapon = part
	if "acog" in scenario.split("_"):
		weapon += "+acog"
	if weapon == "m1911":
		Input.action_press("weapon_2")
		await _sixtieths(2)
		Input.action_release("weapon_2")
	else:
		_player.give_weapon(weapon)
	await _sixtieths(80)
	_player.refill_all_ammo()
	await _sixtieths(30)
	_scenario = scenario
	_t = 0.0
	_label = "idle"


func _release_all() -> void:
	for action in ["fire", "aim", "move_forward", "move_left", "move_right", "sprint", "jump"]:
		Input.action_release(action)
	if _player != null and _player.movement_state() == "Prone":
		Input.action_press("prone")
		Input.action_release("prone")


func _record() -> void:
	if _scenario.is_empty():
		return
	var dt := 1.0 / 60.0
	if _turn != Vector2.ZERO:
		var look := _player.aim_angles()
		_player.set_look(_player.view_yaw() + _turn.x * dt, look.z + _turn.y * dt)
	_t += dt
	var view := _active_view()
	var vm := Transform3D()
	if view != null:
		vm = _rig.transform * view.transform
	var vm_rot := vm.basis.get_euler(EULER_ORDER_YXZ)
	var cam_rot := _camera.global_basis.get_euler(EULER_ORDER_YXZ)
	var aim := _player.aim_angles()
	# Where the sight line points on screen, as a world angle from the camera
	# centre: the weapon is drawn with a 55 degree FOV of its own.
	var sight_dir := (vm.basis * Vector3.FORWARD).normalized()
	var vm_half := tan(deg_to_rad(55.0) * 0.5)
	var world_half := tan(deg_to_rad(_player.camera_fov()) * 0.5)
	var sight := Vector2(
		rad_to_deg(atan(sight_dir.y / -sight_dir.z * world_half / vm_half)),
		rad_to_deg(atan(sight_dir.x / -sight_dir.z * world_half / vm_half))
	)
	var row := [
		_scenario,
		_label,
		"%.4f" % _t,
		"%.4f" % _player.ads_amount(),
		_label_action(),
		_player.magazine(),
		"%.3f" % _player.camera_fov(),
		"%.5f" % vm.origin.x,
		"%.5f" % vm.origin.y,
		"%.5f" % vm.origin.z,
		"%.4f" % rad_to_deg(vm_rot.x),
		"%.4f" % rad_to_deg(vm_rot.y),
		"%.4f" % rad_to_deg(vm_rot.z),
		"%.4f" % rad_to_deg(cam_rot.x),
		"%.4f" % rad_to_deg(cam_rot.y),
		"%.4f" % rad_to_deg(cam_rot.z),
		"%.4f" % rad_to_deg(aim.x),
		"%.4f" % rad_to_deg(aim.y),
		"%.4f" % rad_to_deg(aim.z),
		"%.4f" % sight.x,
		"%.4f" % sight.y,
		"%.3f" % _player.horizontal_speed(),
	]
	var hand := Vector3.ZERO
	if view != null and view.has_node("GripLeft"):
		hand = (view.get_node("GripLeft") as Node3D).position
	# How far the support hand is from the eye: a hand too close fills the screen.
	var hand_eye := (vm * hand).length() if view != null else 0.0
	# Where the magazine well is on screen (-1 to 1 across and up; outside
	# that range it is out of frame), drawn with the weapon's 55 degree FOV.
	var mag := Vector2(9, 9)
	var mag_depth := 0.0
	if view != null and view.has_node("Model/Mesh/Magazine"):
		var well := vm * _rest_in(view, view.get_node("Model/Mesh/Magazine") as Node3D)
		mag_depth = -well.z
		if well.z < -0.01:
			var half := tan(deg_to_rad(55.0) * 0.5)
			mag = Vector2(well.x / -well.z / (half * 16.0 / 9.0), well.y / -well.z / half)
	var tail := [
		"%.4f" % hand.x,
		"%.4f" % hand.y,
		"%.4f" % hand.z,
		"%.4f" % hand_eye,
		"%.3f" % mag.x,
		"%.3f" % mag.y,
		"%.3f" % mag_depth,
		"|".join(_foley),
	]
	row.append_array(tail)
	_foley.clear()
	_file.store_line(",".join(row.map(func(v: Variant) -> String: return str(v))))
	_rows += 1
	if not _frames_dir.is_empty() and _rows % _frame_every == 0:
		_save_frame.call_deferred()


## A node's position in `view`'s space.
func _rest_in(view: Node3D, node: Node3D) -> Vector3:
	return view.global_transform.affine_inverse() * node.global_position


func _save_frame() -> void:
	await RenderingServer.frame_post_draw
	var dir := _frames_dir.path_join(_scenario)
	DirAccess.make_dir_recursive_absolute(dir)
	var image := root.get_viewport().get_texture().get_image()
	image.save_png(dir.path_join("%05d.png" % _frame))
	_frame += 1


func _label_action() -> String:
	if _player.is_reloading():
		return "reload"
	if _player.is_inspecting():
		return "inspect"
	return "ready"


func _active_view() -> Node3D:
	for child in _rig.get_children():
		if child is WeaponView and (child as Node3D).visible:
			return child
	return null


func _mark(label: String) -> void:
	_label = label


func _hold(action: String, sixtieths: int, label: String) -> void:
	_mark(label)
	Input.action_press(action)
	await _sixtieths(sixtieths)
	Input.action_release(action)


func _scenario_ads_m4() -> void:
	await _sixtieths(10)
	_mark("aim_in")
	Input.action_press("aim")
	await _sixtieths(40)
	_mark("aim_out")
	Input.action_release("aim")
	await _sixtieths(40)


func _spray(sixtieths: int, aimed: bool) -> void:
	if aimed:
		Input.action_press("aim")
		await _sixtieths(30)
	await _sixtieths(5)
	await _hold("fire", sixtieths, "fire")
	_mark("release")
	await _sixtieths(60)
	Input.action_release("aim")


func _scenario_ads_acog() -> void:
	await _scenario_ads_m4()


func _scenario_ads_ak47_acog() -> void:
	await _scenario_ads_m4()


func _scenario_ads_pdw9_acog() -> void:
	await _scenario_ads_m4()


func _scenario_spray_ads_acog() -> void:
	await _spray(36, true)


func _scenario_spray_hip_m4() -> void:
	await _spray(72, false)


func _scenario_spray_ads_m4() -> void:
	await _spray(96, true)


func _scenario_spray_ads_ak47() -> void:
	await _spray(96, true)


func _scenario_spray_ads_pdw9() -> void:
	await _spray(96, true)


func _scenario_spray_ads_lsw6() -> void:
	await _spray(96, true)


func _taps(count: int, gap: int) -> void:
	Input.action_press("aim")
	await _sixtieths(30)
	for i in count:
		await _hold("fire", 2, "fire")
		_mark("between")
		await _sixtieths(gap)
	await _sixtieths(30)
	Input.action_release("aim")


func _scenario_taps_m1911() -> void:
	await _taps(5, 13)


func _scenario_burst_br3() -> void:
	await _taps(3, 30)


func _scenario_shot_m40() -> void:
	await _taps(1, 90)


func _scenario_shot_m870() -> void:
	await _taps(2, 50)


## A fast turn (300 degrees a second for a quarter second), then still.
func _flick(aimed: bool) -> void:
	if aimed:
		Input.action_press("aim")
		await _sixtieths(30)
	await _sixtieths(10)
	_mark("turn")
	_turn = Vector2(deg_to_rad(300.0), 0.0)
	await _sixtieths(15)
	_mark("stop")
	_turn = Vector2.ZERO
	await _sixtieths(30)
	_mark("turn_up")
	_turn = Vector2(0.0, deg_to_rad(150.0))
	await _sixtieths(15)
	_mark("stop")
	_turn = Vector2.ZERO
	await _sixtieths(40)
	Input.action_release("aim")


func _scenario_flick_hip() -> void:
	await _flick(false)


func _scenario_flick_ads() -> void:
	await _flick(true)


func _scenario_idle_ads() -> void:
	_mark("hip_idle")
	await _sixtieths(150)
	_mark("ads_idle")
	Input.action_press("aim")
	await _sixtieths(180)


func _scenario_walk_sprint() -> void:
	_mark("walk")
	Input.action_press("move_forward")
	await _sixtieths(60)
	_mark("sprint")
	Input.action_press("sprint")
	await _sixtieths(2)
	Input.action_release("sprint")
	await _sixtieths(90)
	# Firing ends the sprint; the sprint-out delay holds the first shot.
	_mark("sprint_to_fire")
	Input.action_press("fire")
	await _sixtieths(30)
	Input.action_release("fire")
	_mark("walk")
	await _sixtieths(20)
	Input.action_release("move_forward")
	_mark("stop")
	await _sixtieths(40)


func _scenario_strafe() -> void:
	_mark("strafe_left")
	Input.action_press("move_left")
	await _sixtieths(50)
	Input.action_release("move_left")
	_mark("stop")
	await _sixtieths(30)
	_mark("strafe_right")
	Input.action_press("move_right")
	await _sixtieths(50)
	Input.action_release("move_right")
	_mark("stop")
	await _sixtieths(40)


func _scenario_jump() -> void:
	await _sixtieths(10)
	await _hold("jump", 4, "jump")
	_mark("air")
	await _sixtieths(80)
	_mark("landed")
	await _sixtieths(40)


func _reload(empty: bool, aimed: bool, sixtieths: int) -> void:
	if empty:
		# Tap as fast as the weapon allows, so semi-automatics empty too.
		var guard := 0
		while _player.magazine() > 0 and guard < 600:
			Input.action_press("fire")
			await _sixtieths(2)
			Input.action_release("fire")
			await _sixtieths(2)
			guard += 1
		await _sixtieths(20)
	else:
		await _hold("fire", 12, "fire")
		await _sixtieths(30)
	if aimed:
		Input.action_press("aim")
		await _sixtieths(30)
	await _hold("reload", 2, "reload")
	await _sixtieths(sixtieths)
	Input.action_release("aim")


func _scenario_reload_m4() -> void:
	await _reload(false, false, 160)


func _scenario_reload_m4_empty() -> void:
	await _reload(true, false, 190)


func _scenario_reload_ak47_empty() -> void:
	await _reload(true, false, 200)


func _scenario_reload_m40_empty() -> void:
	await _reload(true, false, 240)


func _scenario_reload_lsw6() -> void:
	await _reload(false, false, 320)


func _scenario_reload_m1911_empty() -> void:
	await _reload(true, false, 140)


func _scenario_reload_m870() -> void:
	await _hold("fire", 2, "fire")
	await _sixtieths(60)
	await _hold("fire", 2, "fire")
	await _sixtieths(60)
	await _hold("reload", 2, "reload")
	await _sixtieths(150)


## The newer guns' own reloads (`weapon_clips::Feed`): the CQ-10's magazine
## up the grip and its top knob, the BP-9's knob, the KR-12's magazine
## knocking the old one out, the DM-7's operating rod, the SG-12's box
## slapped home, and the RL-4's rocket.
func _scenario_reload_cq10() -> void:
	await _reload(false, false, 150)


func _scenario_reload_cq10_empty() -> void:
	await _reload(true, false, 180)


func _scenario_reload_bp9() -> void:
	await _reload(false, false, 130)


func _scenario_reload_bp9_empty() -> void:
	await _reload(true, false, 160)


func _scenario_reload_kr12() -> void:
	await _reload(false, false, 180)


func _scenario_reload_kr12_empty() -> void:
	await _reload(true, false, 210)


func _scenario_reload_dm7_empty() -> void:
	await _reload(true, false, 220)


func _scenario_reload_sg12() -> void:
	await _reload(false, false, 170)


func _scenario_reload_sg12_empty() -> void:
	await _reload(true, false, 200)


func _scenario_reload_rl4() -> void:
	await _reload(true, false, 220)


func _scenario_reload_ads_m4() -> void:
	await _reload(false, true, 160)


## A tactical reload fired out of as soon as the rules allow: the trigger is
## held from the seat onward.
func _scenario_reload_fire_m4() -> void:
	await _hold("fire", 12, "fire")
	await _sixtieths(30)
	await _hold("reload", 2, "reload")
	await _sixtieths(80)
	_mark("fire")
	Input.action_press("fire")
	await _sixtieths(60)
	Input.action_release("fire")
	_mark("release")
	await _sixtieths(40)


## Down to prone, then the low crawl forward at the hip: the weapon rolls on
## its side and rocks with each elbow pull while the support hand reaches out
## and drags; then aimed, crawling with the gun up.
func _crawl() -> void:
	await _hold("prone", 2, "prone")
	await _sixtieths(60)
	_mark("crawl")
	Input.action_press("move_forward")
	await _sixtieths(150)
	_mark("crawl_aimed")
	Input.action_press("aim")
	await _sixtieths(60)
	Input.action_release("aim")
	Input.action_release("move_forward")
	await _sixtieths(30)
	await _hold("prone", 2, "stand")
	await _sixtieths(70)


func _scenario_crawl() -> void:
	await _crawl()


func _scenario_crawl_m1911() -> void:
	await _crawl()


## A Zombies drink (`Raise::Drink`): the weapon goes down, the bottle comes
## up, the cap is flicked off, three gulps, the bottle is tossed, and the
## weapon is drawn.
## Reviving a downed teammate in Zombies: the weapon down, the left hand on
## them, the right jabbing the syrette in and holding it.
func _scenario_revive() -> void:
	_mark("revive")
	_player.debug_revive(true)
	await _sixtieths(150)
	_player.debug_revive(false)
	_mark("up")
	await _sixtieths(60)


## Defusing Search and Destroy's charge: cutters on its wires, one snip and
## then the other.
func _scenario_defuse() -> void:
	_mark("defuse")
	_player.debug_defuse(true)
	await _sixtieths(200)
	_player.debug_defuse(false)
	_mark("up")
	await _sixtieths(60)


func _scenario_drink() -> void:
	_mark("drink")
	_player.debug_drink("bulwark")
	await _sixtieths(200)


func _scenario_drink_m1911() -> void:
	_mark("drink")
	_player.debug_drink("long_stride")
	await _sixtieths(200)


func _scenario_switch() -> void:
	await _hold("weapon_2", 2, "holster_m4")
	await _sixtieths(70)
	await _hold("weapon_1", 2, "holster_m1911")
	await _sixtieths(80)


func _scenario_inspect() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(200)


## Each class inspects its own way (`weapon_clips::inspect_pose`).
func _scenario_inspect_m1911() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(200)


func _scenario_inspect_m870() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(200)


func _scenario_inspect_m40() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(220)


func _scenario_inspect_cq10() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(220)


func _scenario_inspect_bp9() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(200)


func _scenario_inspect_lsw6() -> void:
	await _hold("inspect", 2, "inspect")
	await _sixtieths(230)


## A weapon off the rack comes up with a first raise, working the action.
func _first_raise(weapon: String) -> void:
	await _sixtieths(10)
	_mark("first_raise")
	_player.give_weapon(weapon)
	await _sixtieths(110)


func _scenario_first_raise() -> void:
	await _first_raise("ak47")


func _scenario_first_raise_bolt() -> void:
	await _first_raise("m40")


func _scenario_first_raise_pump() -> void:
	await _first_raise("m870")


## A frag cooked for most of a second, then thrown.
func _scenario_throw() -> void:
	_player.restock_lethals()
	await _sixtieths(10)
	await _hold("lethal", 50, "cook")
	_mark("throw")
	await _sixtieths(60)


func _scenario_throw_quick() -> void:
	_player.restock_lethals()
	await _sixtieths(10)
	await _hold("lethal", 2, "throw")
	await _sixtieths(70)


func _scenario_slide() -> void:
	Input.action_press("move_forward")
	await _hold("sprint", 2, "sprint")
	await _sixtieths(45)
	await _hold("crouch", 2, "slide")
	await _sixtieths(60)
	Input.action_release("move_forward")
	_mark("stop")
	await _sixtieths(40)


func _scenario_crouch() -> void:
	await _hold("crouch", 2, "crouch")
	await _sixtieths(40)
	_mark("crouch_walk")
	Input.action_press("move_forward")
	await _sixtieths(40)
	Input.action_release("move_forward")
	await _hold("crouch", 2, "stand")
	await _sixtieths(40)


## A wall `height` metres tall and `depth` deep, `ahead` metres in front of
## the spawn, for the climbs; freed when the scenario ends.
func _obstacle(height: float, depth: float, ahead: float) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(4.0, height, depth)
	shape.shape = box
	body.add_child(shape)
	var mesh := MeshInstance3D.new()
	var cube := BoxMesh.new()
	cube.size = box.size
	mesh.mesh = cube
	body.add_child(mesh)
	root.add_child(body)
	body.global_position = Vector3(0, height * 0.5, 20.0 - ahead - depth * 0.5)
	await _sixtieths(2)
	return body


func _climb(height: float, depth: float, sprint: bool) -> void:
	var wall := await _obstacle(height, depth, 3.0)
	Input.action_press("move_forward")
	if sprint:
		await _hold("sprint", 2, "sprint")
	while _player.global_position.z > 17.0 + 0.7:
		await _sixtieths(1)
	await _hold("jump", 6, "climb")
	await _sixtieths(70)
	Input.action_release("move_forward")
	_mark("stop")
	await _sixtieths(30)
	wall.queue_free()


func _scenario_vault() -> void:
	await _climb(1.0, 0.3, true)


func _scenario_mantle() -> void:
	await _climb(1.2, 3.0, false)


## Shot from the right: the weapon is knocked away and comes back.
func _scenario_hit() -> void:
	await _sixtieths(10)
	_mark("hit")
	_player.take_damage(25.0, _player.global_position + Vector3(10, 1.5, 0))
	await _sixtieths(50)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
