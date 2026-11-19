extends SceneTree
## Headless test of the combat effects pools (`rust/game/src/effects.rs`):
## bullet holes stay on the wall and recycle oldest first within the preset's
## budget; a round into someone close to a wall leaves blood spatter on it,
## and one with no wall behind does not; other players' traced rounds draw
## their impacts; impacts hide again once their particles are done; and
## characters draw on the layer decals skip.
## Run from the repo root: godot --headless --path godot --script res://tests/combat_fx_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
const SOLDIER_SCENE := preload("res://scenes/characters/soldier.tscn")
## `effects::NO_DECALS_LAYER`.
const NO_DECALS_LAYER := 1 << 1

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings: Node = root.get_node("Settings")
	settings.path = "user://combat_fx_test_settings.cfg"
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(80, 1, 80))
	# A wall 10 m ahead of the player (its face at z = -9.75).
	_solid(level, Vector3(0, 2, -10), Vector3(20, 4, 0.5))
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 0)
	level.add_child(player)
	await _frames(10)
	var effects: Node = get_first_node_in_group("combat_effects")
	_check(effects != null, "the local player adds the effects node")
	if effects == null:
		_finish()
		return

	# Holes stay, up to the budget, on every preset.
	for preset in 3:
		settings.set_graphics_preset(preset)
		var budget: Dictionary = effects.budget()
		for i in budget["holes"] + 10:
			effects.impact(Vector3(i * 0.05 - 5.0, 1.5, -9.75), Vector3(0, 0, 1), "concrete", true)
		await _frames(2)
		_check(
			effects.holes_kept() == budget["holes"],
			"preset %d keeps %d holes (kept %d)" % [preset, budget["holes"], effects.holes_kept()]
		)
		_check(
			effects.impacts_visible() <= budget["impacts"],
			"preset %d plays at most %d impacts at once" % [preset, budget["impacts"]]
		)
	# Lowering the preset frees the extra holes on the next one placed.
	settings.set_graphics_preset(0)
	effects.impact(Vector3(0, 1, -9.75), Vector3(0, 0, 1), "metal", true)
	_check(effects.holes_kept() == 16, "Low trims holes back to 16")
	settings.set_graphics_preset(1)

	# A round leaving cover blows out a wider hole than one going in.
	effects.impact(Vector3(2, 1, -9.75), Vector3(0, 0, 1), "wood", true)
	var entry := _newest_decal(effects)
	effects.exit_impact(Vector3(3, 1, -9.75), Vector3(0, 0, 1), "concrete", true)
	var exit := _newest_decal(effects)
	_check(entry != null and exit != null, "entry and exit holes are kept")
	if entry != null and exit != null:
		_check(exit.size.x > entry.size.x * 1.2, "the exit hole is wider")
		_check(entry.texture_orm != null, "holes carry their own roughness")
		_check(entry.cull_mask & NO_DECALS_LAYER == 0, "holes skip the characters' layer")

	# Blood: spatter on the wall a metre behind the target, none in the open.
	var placed: int = effects.splats_placed()
	effects.blood(Vector3(0, 1.3, -8.75), Vector3(0, 0, -1))
	_check(effects.splats_placed() == placed + 1, "a hit a metre from a wall spatters it")
	effects.blood(Vector3(0, 1.3, -5.0), Vector3(0, 0, -1))
	_check(effects.splats_placed() == placed + 1, "no spatter with the wall 4.75 m away")
	effects.blood(Vector3(0, 1.3, -8.75), Vector3(0, 0, 1))
	_check(effects.splats_placed() == placed + 1, "no spatter when the round flies away from it")

	# Another player's round into the wall draws its impact and keeps a hole.
	settings.set_graphics_preset(2)
	var holes: int = effects.holes_kept()
	var tracer := player.get_node("ShotTracer") as ShotTracer
	tracer.trace_test_shot("m4", Vector3(3, 1.6, 0), Vector3(0, 0, -1))
	_check(effects.holes_kept() == holes + 1, "a traced remote round leaves a hole")

	# Other players' tracers: every third round of an M4, every round of the
	# bolt rifle, none from the shotgun; never more in flight than the budget.
	var started: int = effects.tracers_started()
	for i in 6:
		tracer.trace_test_shot("m4", Vector3(-3, 1.6, 0), Vector3(0, 0, -1))
	_check(effects.tracers_started() == started + 2, "an M4 shows a tracer every third round")
	started = effects.tracers_started()
	tracer.trace_test_shot("m40", Vector3(-3, 1.6, 0), Vector3(0, 0, -1))
	tracer.trace_test_shot("m870", Vector3(-3, 1.6, 0), Vector3(0, 0, -1))
	_check(
		effects.tracers_started() == started + 1, "the bolt rifle traces every round, buckshot none"
	)
	for i in 40:
		effects.tracer(Vector3(0, 1.5, 0), Vector3(0, 1.5, -100))
	_check(
		effects.tracers_flying() <= effects.budget()["tracers"],
		"tracers in flight stay within the budget"
	)

	# An explosion plays from the pool, leaves one scorch per spot, and a
	# grenade's blast scene uses it.
	var played: int = effects.explosions_played()
	var scorched: int = effects.scorches_kept()
	effects.explosion(Vector3(4, 0.0, -4), Vector3.UP, 1.0)
	effects.explosion(Vector3(4.3, 0.0, -4), Vector3.UP, 1.0)
	_check(effects.explosions_played() == played + 2, "explosions play")
	_check(effects.scorches_kept() == scorched + 1, "a second blast on the spot reuses its scorch")
	var explosion: Node = null
	for child: Node in effects.get_children():
		if child.scene_file_path.ends_with("explosion.tscn"):
			explosion = child
			break
	_check(explosion != null, "the explosion is pooled under the effects node")
	if explosion != null:
		var chips := (explosion.get_node("Debris") as GPUParticles3D).draw_pass_1
		_check(
			chips != null and chips.get_faces().size() > 36,
			"explosion debris draws the modelled chip, not a box"
		)
		var dirt := explosion.get_node("Dirt") as GPUParticles3D
		_check(
			absf(float(dirt.get_instance_shader_parameter(&"ground_y"))) < 0.05,
			"the dirt thrown up fades in above the ground the blast went off on"
		)
	var blast := (load("res://scenes/equipment/frag_blast.tscn") as PackedScene).instantiate()
	blast.position = Vector3(-4, 0.0, -4)
	level.add_child(blast)
	_check(effects.explosions_played() == played + 3, "a frag's blast plays the pooled explosion")
	for i in 12:
		effects.explosion(Vector3(i * 3.0 - 18.0, 0.0, -6), Vector3.UP, 1.0)
	_check(
		effects.scorches_kept() <= effects.budget()["scorches"], "scorches stay within the budget"
	)

	# Fires burn from a pool, burn out, and can be put out early.
	var fire: Node3D = effects.fire(Vector3(-6, 0, -4), 1.0, 0.5)
	_check(fire != null and effects.fires_burning() >= 1, "a fire burns")
	for i in effects.budget()["fires"] + 2:
		effects.fire(Vector3(i * 2.0, 0, -6), 1.0, 30.0)
	_check(effects.fires_burning() <= effects.budget()["fires"], "fires stay within the budget")
	for f: Node in effects.get_children():
		if f.has_method("stop"):
			f.stop()

	# Smoke clouds hang from a pool; a late one starts grown; a dropped one
	# fades. A flashbang's pop plays from its own pool.
	var cloud: Node3D = effects.smoke(Vector3(4, 0, -8), 0.0)
	_check(cloud != null and effects.smokes_hanging() == 1, "a smoke cloud hangs")
	var late: Node3D = effects.smoke(Vector3(-4, 0, -8), 8.0)
	_check(late != null and is_equal_approx(late.radius(), 5.0), "a late cloud starts full grown")
	for i in effects.budget()["smokes"] + 2:
		effects.smoke(Vector3(i * 3.0, 0, -10), 0.0)
	_check(effects.smokes_hanging() <= effects.budget()["smokes"], "clouds stay within the budget")
	for child in effects.get_children():
		if child.has_method("dissipate"):
			child.dissipate()
	var pops: int = effects.flashes_played()
	effects.flash_pop(Vector3(0, 0, -6))
	_check(effects.flashes_played() == pops + 1, "a flashbang pops")
	await _frames(60)
	_check(effects.smokes_hanging() == 0, "dropped clouds fade away")

	# Fired cases fly, clink, and come to rest on the floor, within budget.
	var clinks: int = effects.clinks_played()
	effects.throw_test_case("Case556", Vector3(0, 1.2, -3), Vector3(2, 1, 0))
	await _frames(240)
	_check(effects.cases_resting() >= 1, "a case comes to rest")
	_check(effects.clinks_played() > clinks, "a landing case clinks")
	var lying: Node3D = null
	for child in effects.get_children():
		if child is MeshInstance3D and child.mesh != null and child.mesh.get_aabb().size.y > 0.04:
			lying = child
	_check(lying != null and absf(lying.global_position.y - 0.005) < 0.01, "it lies on the floor")
	for i in 60:
		effects.throw_test_case("Case9mm", Vector3(0, 1.2, -3), Vector3(1, 1, 0))
	_check(effects.cases_kept() <= effects.budget()["cases"], "cases stay within the budget")
	# Other players' weapons throw theirs.
	var thrown: int = effects.cases_thrown()
	var rifleman := SOLDIER_SCENE.instantiate()
	level.add_child(rifleman)
	rifleman.global_position = Vector3(2, 0, -4)
	rifleman.drive({"weapon": "m4", "on_floor": true}, 1.0 / 60.0)
	await _frames(2)
	rifleman.shoot()
	_check(effects.cases_thrown() == thrown + 1, "a third-person shot throws a case")
	# A shooter in view lights the scene with the shot; one behind the wall
	# does not light the near side of it.
	var light := rifleman.weapon_node().get_node("Muzzle/FlashLight") as OmniLight3D
	_check(light.visible, "a shot in view flashes its light")
	var hidden := SOLDIER_SCENE.instantiate()
	level.add_child(hidden)
	hidden.global_position = Vector3(0, 0, -13)
	hidden.drive({"weapon": "m4", "on_floor": true}, 1.0 / 60.0)
	await _frames(2)
	hidden.shoot()
	var behind := hidden.weapon_node().get_node("Muzzle/FlashLight") as OmniLight3D
	_check(not behind.visible, "a shot behind a wall keeps its light off")

	# Impacts hide once their particles are done, so an idle pool draws nothing.
	await _frames(240)
	_check(effects.impacts_visible() == 0, "finished impacts hide")
	_check(effects.tracers_flying() == 0, "finished tracers hide")
	await _frames(600)
	_check(effects.fires_burning() == 0, "put-out fires die down")

	# Characters are kept off the decal layer.
	var soldier := SOLDIER_SCENE.instantiate()
	level.add_child(soldier)
	await _frames(2)
	var meshes := soldier.find_children("*", "VisualInstance3D", true, false)
	var clean := not meshes.is_empty()
	for mesh: VisualInstance3D in meshes:
		clean = clean and mesh.layers == NO_DECALS_LAYER
	_check(clean, "every soldier mesh is on the layer decals skip")

	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(settings.path))
	_finish()


func _finish() -> void:
	if _failures.is_empty():
		print("combat_fx_test: OK")
		quit(0)
	else:
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)


## The decal placed last: the effects node adds them in order and moves a
## reused one to the end of the ring, so the latest position wins.
func _newest_decal(effects: Node) -> Decal:
	var newest: Decal = null
	for child in effects.get_children():
		if child is Decal:
			newest = child
	return newest


func _check(condition: bool, label: String) -> void:
	if not condition:
		_failures.append(label)


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


func _solid(parent: Node, position: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	body.position = position
	parent.add_child(body)
	return body
