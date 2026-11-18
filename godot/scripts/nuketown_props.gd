extends Node3D
## Source-authored script entities, per-piece destruction, and loose physics.
## Only the server (or offline game) changes health. Clients present reliable stages.

const ROOT := "res://assets/maps/nuketown/props/"
const BODY := preload("res://scripts/nuketown_prop_body.gd")
const ANIMATION := preload("res://scripts/nuketown_prop_animation.gd")
const FX := preload("res://scripts/nuketown_prop_fx.gd")
const PART_STRIDE := 32
const BLAST_RADIUS := 6.5024
const BLAST_DAMAGE := 300.0
const BLAST_OUTER_DAMAGE := 75.0
const MANNEQUINS_PER_MATCH := 25
## Scripted models that stay solid, by targetname. The population sign
## (`nuketown_tv`) only has its screen driven by script; the rest of it is a
## big, solid billboard in the original map.
const SOLID_DECORATIONS: Array[String] = ["nuketown_tv"]

var pieces: Dictionary = {}
var entities: Dictionary = {}
var manifest: Dictionary = {}
var behavior: Dictionary = {}
var population := 1
var _definitions: Dictionary = {}
var _events: Array[Dictionary] = []
var _explosions: Array[Dictionary] = []
var _models: Dictionary = {}
var _shapes: Dictionary = {}
var _debris: Node3D
var _dials: Array[Node3D] = []
var _spinner: Node3D
var _needle: Node3D
var _animated: Array = []
var _clocks: Array[Dictionary] = []
var _dial_value := 0.0
var _clock := 0.0
var _server := false
var _ended := false


func _ready() -> void:
	add_to_group("world_props")
	var ancestor := get_parent()
	while ancestor != null:
		if ancestor is NetServer:
			_server = true
		ancestor = ancestor.get_parent()
	manifest = _read_json(ROOT + "manifest.json")
	behavior = _read_json(ROOT + "behavior.json")
	for definition: Dictionary in behavior.get("destructibles", []):
		_definitions[definition.name] = definition
	_debris = Node3D.new()
	_debris.name = "Debris"
	add_child(_debris)
	_build_entities()
	_build_loose()
	_hide_baked_copies()
	set_population(1 if not _server else 0)


func authority() -> bool:
	if _server:
		return true
	if Net.is_online():
		return false
	for child in get_parent().get_children():
		if child is NetClient:
			return false
	return true


func _physics_process(delta: float) -> void:
	_clock += delta
	if authority():
		for id: int in pieces:
			var record: Dictionary = pieces[id]
			if float(record.get("burn_left", -1.0)) > 0.0:
				record.burn_left -= delta
				if record.burn_left <= 0.0:
					damage_prop(id, 10000.0, Vector3.UP, record.attacker, record.weapon)
		if not _server:
			_apply_offline_explosions()
	_dial_value = move_toward(_dial_value, float(population), delta * 2.0)
	for i in _dials.size():
		var dial := _dials[i]
		var base: Basis = dial.get_meta("base_basis")
		var digit := fmod(_dial_value, 10.0) if i == 1 else floorf(_dial_value / 10.0)
		dial.basis = base * Basis(Vector3.RIGHT, deg_to_rad(36.0 * (digit + 1.0)))
	if _spinner != null:
		_spinner.rotate_y(deg_to_rad(90.0) * delta)
	if _needle != null:
		var progress := clampf(_clock / 600.0, 0.0, 1.0)
		if Net.is_online() and not _server:
			var info: Dictionary = Net.client().match_info()
			progress = maxf(progress, float(info.get("progress", 0.0)))
		_needle.basis = Basis(Vector3.BACK, deg_to_rad(-lerpf(128.0, -95.0, progress)))
	for entry: Dictionary in _animated:
		if entry.loop or _ended:
			entry.player.advance(delta)
	for hand: Dictionary in _clocks:
		hand.node.basis = hand.basis * Basis(Vector3.RIGHT, -TAU * _clock / hand.period)
	var phase := ""
	if _server:
		phase = String(get_parent().get_parent().match_phase())
	elif Net.is_online():
		phase = String(Net.client().match_info().get("phase", ""))
	if phase == "ended" and not _ended:
		play_outro()


func _build_entities() -> void:
	var mannequins: Array[int] = []
	for item: Dictionary in manifest.get("entities", []):
		if String(item.destructibledef).begins_with("dest_nt_nuked_"):
			mannequins.append(int(item.id))
	# Source scripts randomly retain ~25. A seeded shuffle gives every peer the
	# same layout without sending cosmetic placement randomness over the wire.
	var rng := RandomNumberGenerator.new()
	rng.seed = 2025
	for i in range(mannequins.size() - 1, 0, -1):
		var j := rng.randi_range(0, i)
		var old := mannequins[i]
		mannequins[i] = mannequins[j]
		mannequins[j] = old
	mannequins.resize(mini(MANNEQUINS_PER_MATCH, mannequins.size()))
	for item: Dictionary in manifest.get("entities", []):
		var modes := String(item.source_keys.get("script_gameobjectname", ""))
		if not modes.is_empty() and not "dm" in modes.split(" ") and not "tdm" in modes.split(" "):
			continue
		# Exploder models are the wrecked bomb stacks at the Search and Destroy
		# and Demolition sites. CoD hides them until that site's bomb goes off,
		# which never happens in the modes Nuketown supports.
		if item.targetname == "exploder" and item.source_keys.has("script_exploder"):
			continue
		if (
			String(item.destructibledef).begins_with("dest_nt_nuked_")
			and not int(item.id) in mannequins
		):
			continue
		var definition: Dictionary = _definitions.get(item.destructibledef, {})
		var model := String(definition.get("model", item.model))
		var instance := _model(model)
		if instance == null:
			push_error("Missing interactive model: " + model)
			continue
		instance.name = "Entity_%d_%s" % [int(item.id), model]
		add_child(instance)
		instance.position = _vec(item.position)
		var angles := _vec(item.rotation_degrees)
		instance.basis = _entity_basis(angles)
		entities[int(item.id)] = {"node": instance, "item": item, "model": model}
		var meshes := _meshes(instance)
		if not definition.is_empty():
			_build_destructible(item, instance, meshes, definition)
		else:
			_build_decoration(item, instance, meshes)
		_setup_animation(instance, model, int(item.id))
	# This client model is normally an outro copy, but its pristine frame also
	# restores the showroom pane missing from the render-only BSP export.
	for skipped: Dictionary in manifest.get("skipped", []):
		if skipped.get("targetname", "") == "nuke_display_glass_client":
			var keys: Dictionary = skipped.source_keys
			var pane := _model(skipped.model)
			add_child(pane)
			pane.position = _source_vector(String(keys.origin))
			_setup_animation(pane, skipped.model, -1)
			for mesh: MeshInstance3D in _meshes(pane):
				if mesh.visible:
					_collider(mesh, -1, "glass")


func _build_destructible(
	item: Dictionary, root: Node3D, meshes: Array, definition: Dictionary
) -> void:
	var all_bones: Array = []
	for bone: Dictionary in definition.model_info.bones:
		all_bones.append(bone.name)
	all_bones.sort_custom(func(a: String, b: String) -> bool: return a.length() > b.length())
	var by_bone: Dictionary = {}
	for mesh: MeshInstance3D in meshes:
		for bone: String in all_bones:
			if String(mesh.name) == bone or String(mesh.name).begins_with(bone + "_"):
				if not by_bone.has(bone):
					by_bone[bone] = []
				by_bone[bone].append(mesh)
				break
	for index in definition.pieces.size():
		var spec: Dictionary = definition.pieces[index]
		var id: int = int(item.id) * PART_STRIDE + index
		var states: Array[Dictionary] = []
		for state: Dictionary in spec.stages:
			if state.active:
				states.append(state)
		var record := {
			"root": root,
			"spec": spec,
			"states": states,
			"stage": 0,
			"health": float(spec.health),
			"bones": by_bone,
			"bodies": {},
			"entity": int(item.id),
			"index": index,
			"car": String(definition.name).begins_with("veh_"),
			"impulse": Vector3.ZERO,
			"attacker": -1,
			"weapon": 0,
			"burn_left": -1.0,
			"fx": [],
			"kind": "destructible"
		}
		pieces[id] = record
		for bone: String in spec.hide_bones.names:
			for mesh: MeshInstance3D in by_bone.get(bone, []):
				var body := _collider(mesh, id, "metal" if record.car else "plastic")
				if not record.bodies.has(bone):
					record.bodies[bone] = []
				record.bodies[bone].append(body)
		_show_stage(record, 0)


func _build_decoration(item: Dictionary, root: Node3D, meshes: Array) -> void:
	match String(item.targetname):
		"counter_tens", "counter_ones":
			root.set_meta("base_basis", root.basis)
			if item.targetname == "counter_tens":
				_dials.push_front(root)
			else:
				_dials.append(root)
		"nuketown_sign_topper":
			_spinner = root
		"nuketown_sign_needle":
			_needle = root
	if String(item.targetname) in ["second_hand", "minute_hand", "hour_hand"]:
		var period: float = {"second_hand": 60.0, "minute_hand": 3600.0, "hour_hand": 43200.0}[String(
			item.targetname
		)]
		_clocks.append({"node": root, "basis": root.basis, "period": period})
	# Animated decorations and clock hands are visual. Solid script models
	# retain collision; the tiny hands/dials must not become invisible walls.
	var solid: bool = item.targetname.is_empty() or item.targetname in SOLID_DECORATIONS
	if solid and not String(item.model).contains("clock"):
		for mesh: MeshInstance3D in meshes:
			if mesh.mesh.get_aabb().size.length() > 0.45:
				_collider(mesh, -1, "metal")


func _build_loose() -> void:
	for item: Dictionary in manifest.get("loose", []):
		var root := _model(item.model)
		if root == null:
			continue
		add_child(root)
		root.name = "Loose_%d" % int(item.id)
		root.position = _vec(item.position)
		var r := _vec(item.rotation_degrees)
		root.basis = _entity_basis(Vector3(r.y, r.z, r.x)).scaled(_vec(item.scale))
		var id := int(item.id) * PART_STRIDE
		var record := {
			"root": root,
			"kind": "loose",
			"stage": 0,
			"health": 1.0,
			"impulse": Vector3.ZERO,
			"fx": []
		}
		pieces[id] = record
		for mesh: MeshInstance3D in _meshes(root):
			_collider(mesh, id, "metal" if String(item.model).contains("can") else "plastic")


func damage_prop(id: int, amount: float, impulse: Vector3, attacker: int, weapon: int) -> void:
	if not authority() or not pieces.has(id) or not is_finite(amount) or amount <= 0.0:
		return
	var record: Dictionary = pieces[id]
	if record.kind != "destructible":
		if record.stage == 0:
			record.health -= amount
			if record.health <= 0.0:
				_change(id, 1, impulse, attacker, weapon)
		return
	_damage_piece(id, amount, impulse, attacker, weapon, "bullet")


func _damage_piece(
	id: int, amount: float, impulse: Vector3, attacker: int, weapon: int, damage_type: String
) -> void:
	var record: Dictionary = pieces[id]
	var scaled := amount * float(record.spec.damage_scales.get(damage_type, 1.0))
	record.health -= scaled
	record.attacker = attacker
	record.weapon = weapon
	var transfer := float(record.spec.damage_scales.entity_transfer)
	if record.car and record.index != 0 and transfer > 0.0:
		_damage_piece(
			record.entity * PART_STRIDE, amount * transfer, impulse, attacker, weapon, damage_type
		)
	var stage: int = record.stage
	while stage < record.states.size():
		var state: Dictionary = record.states[stage]
		var threshold := float(state.break_health_fraction)
		if threshold < 0.0 or record.health > float(record.spec.health) * threshold:
			break
		# A final wreck is persistent; a final detachable part leaves its stand.
		if stage == record.states.size() - 1 and (state.spawn_models as Array).is_empty():
			break
		stage += 1
		_change(id, stage, impulse, attacker, weapon)


func _change(id: int, stage: int, impulse: Vector3, attacker: int, weapon: int) -> void:
	var record: Dictionary = pieces[id]
	var previous: int = record.stage
	apply_state(id, stage, impulse)
	_events.append({"id": id, "stage": stage, "impulse": impulse})
	if record.kind == "destructible" and previous < record.states.size():
		var state: Dictionary = record.states[previous]
		if state.break_notify == "destructible_car_explosion":
			# Raise the origin above the wreck so its own body does not occlude damage.
			_queue_blast(record.root.global_position + Vector3.UP * 1.5, attacker, weapon)
			for other_id: int in pieces:
				var other: Dictionary = pieces[other_id]
				if other.get("entity", -2) == record.entity and other.index != 0:
					_damage_piece(other_id, 10000.0, impulse, attacker, weapon, "explosive")


func apply_state(id: int, stage: int, impulse: Vector3) -> void:
	if not pieces.has(id) or stage < 0 or stage > 5:
		return
	var record: Dictionary = pieces[id]
	var previous: int = record.stage
	if previous == stage:
		return
	record.stage = stage
	record.impulse = impulse
	if stage == 0:
		record.root.visible = true
		for child in _debris.get_children():
			if int(child.get_meta("prop_id", -1)) == id:
				child.queue_free()
	if record.kind == "loose":
		record.root.visible = stage == 0
		_set_root_collision(record.root, stage == 0)
		if stage > 0 and impulse != Vector3.ZERO:
			_spawn_debris(_meshes(record.root), impulse, id)
	else:
		var old_meshes: Array = []
		if previous < record.states.size():
			var old_bone = record.states[previous].show_bone.name
			if old_bone != null:
				old_meshes = record.bones.get(old_bone, [])
		_show_stage(record, stage)
		if previous < stage and previous < record.states.size() and impulse != Vector3.ZERO:
			var old_state: Dictionary = record.states[previous]
			if (
				not (old_state.spawn_models as Array).is_empty()
				or record.spec.phys_constraints != null
			):
				_spawn_debris(old_meshes, impulse, id)
			if old_state.break_notify == "destructible_car_explosion":
				FX.burst(_debris, record.root.global_position + Vector3.UP)
		_refresh_fire(record)


func _show_stage(record: Dictionary, stage: int) -> void:
	var visible_bone: String = ""
	if stage < record.states.size():
		visible_bone = (
			String(record.states[stage].show_bone.name)
			if record.states[stage].show_bone.name != null
			else ""
		)
	for bone: String in record.spec.hide_bones.names:
		for mesh: MeshInstance3D in record.bones.get(bone, []):
			mesh.visible = bone == visible_bone
		for body: StaticBody3D in record.bodies.get(bone, []):
			body.collision_layer = 1 if bone == visible_bone else 0


func _refresh_fire(record: Dictionary) -> void:
	for effect: Node in record.fx:
		if is_instance_valid(effect):
			effect.queue_free()
	record.fx.clear()
	if not record.car or record.index != 0 or record.stage == 0:
		return
	if record.stage >= 1:
		record.fx.append(FX.plume(record.root, false))
	if record.stage >= 2:
		record.fx.append(FX.plume(record.root, true))
		if record.stage == 2 and authority():
			# A burning car must eventually explode without further bullets.
			record.burn_left = 7.0 + float(record.entity % 3)
	else:
		record.burn_left = -1.0


func _queue_blast(point: Vector3, attacker: int, weapon: int) -> void:
	_explosions.append(
		{
			"position": point,
			"radius": BLAST_RADIUS,
			"damage": BLAST_DAMAGE,
			"outer_damage": BLAST_OUTER_DAMAGE,
			"attacker": attacker,
			"weapon": weapon
		}
	)
	# Mark the source wreck before applying a blast, so chain reactions cannot
	# re-enter its explosion. Apply each neighbouring car's body only once.
	for id: int in pieces.keys():
		var record: Dictionary = pieces[id]
		if (
			record.kind != "destructible"
			or not record.car
			or record.index != 0
			or record.stage >= 3
		):
			continue
		var target: Vector3 = record.root.global_position + Vector3.UP * 0.7
		var distance := point.distance_to(target)
		if distance < BLAST_RADIUS and _clear_blast(point, target, record.root):
			_damage_piece(
				id,
				lerpf(BLAST_DAMAGE, BLAST_OUTER_DAMAGE, distance / BLAST_RADIUS),
				target - point,
				attacker,
				weapon,
				"explosive"
			)


func _clear_blast(from: Vector3, to: Vector3, target: Node3D) -> bool:
	var query := PhysicsRayQueryParameters3D.create(from, to, 1)
	var hit := get_world_3d().direct_space_state.intersect_ray(query)
	return hit.is_empty() or target.is_ancestor_of(hit.collider)


func _apply_offline_explosions() -> void:
	for explosion: Dictionary in drain_explosions():
		var player := get_parent().get_node_or_null("Player") as FpsPlayer
		if player == null:
			continue
		var target := player.global_position + Vector3.UP
		var distance := target.distance_to(explosion.position)
		if distance < explosion.radius and _clear_blast(explosion.position, target, player):
			player.take_damage(
				lerpf(explosion.damage, explosion.outer_damage, distance / explosion.radius),
				explosion.position
			)


func drain_events() -> Array[Dictionary]:
	var events := _events
	_events = []
	return events


func drain_explosions() -> Array[Dictionary]:
	var explosions := _explosions
	_explosions = []
	return explosions


func snapshot_states() -> Array[Dictionary]:
	var result: Array[Dictionary] = []
	for id: int in pieces:
		var record: Dictionary = pieces[id]
		result.append({"id": id, "stage": record.stage, "impulse": Vector3.ZERO})
	return result


func reset_props() -> void:
	for child in _debris.get_children():
		child.queue_free()
	for id: int in pieces:
		var record: Dictionary = pieces[id]
		apply_state(id, 0, Vector3.ZERO)
		record.health = float(record.spec.health) if record.kind == "destructible" else 1.0
		record.burn_left = -1.0
		record.impulse = Vector3.ZERO
	_events.clear()
	_explosions.clear()
	_clock = 0.0
	_ended = false
	for entry: Dictionary in _animated:
		entry.player.play(entry.clip, entry.loop)
	for record: Dictionary in pieces.values():
		if record.kind == "destructible":
			_show_stage(record, 0)


func set_population(count: int) -> void:
	population = clampi(count, 0, 99)


func _spawn_debris(meshes: Array, impulse: Vector3, id: int) -> void:
	if meshes.is_empty() or _server:
		return
	var body := RigidBody3D.new()
	body.collision_layer = 0
	body.collision_mask = 1
	body.mass = 0.4
	body.set_meta("prop_id", id)
	_debris.add_child(body)
	body.global_transform = (meshes[0] as MeshInstance3D).global_transform
	for source: MeshInstance3D in meshes:
		var copy := MeshInstance3D.new()
		copy.mesh = source.mesh
		body.add_child(copy)
		copy.global_transform = source.global_transform
		var shape := CollisionShape3D.new()
		shape.shape = _convex(source.mesh)
		copy.add_child(shape)
		# CollisionShape must be a direct child of a physics body.
		shape.reparent(body)
	body.linear_velocity = (
		impulse.normalized() * clampf(impulse.length() * 0.08, 1.0, 5.0) + Vector3.UP * 1.5
	)
	body.angular_velocity = Vector3(2, 3, 1)
	var timer := Timer.new()
	timer.one_shot = true
	timer.wait_time = 12.0
	body.add_child(timer)
	timer.timeout.connect(body.queue_free)
	timer.start()


func _collider(mesh: MeshInstance3D, id: int, surface: String) -> StaticBody3D:
	var body := StaticBody3D.new()
	if id >= 0:
		body.set_script(BODY)
		body.set("manager", self)
		body.set("prop_id", id)
		body.set_meta("prop_id", id)
	body.set_meta("surface", surface)
	mesh.add_child(body)
	var shape := CollisionShape3D.new()
	shape.shape = _trimesh(mesh.mesh)
	body.add_child(shape)
	return body


func _convex(mesh: Mesh) -> Shape3D:
	var key := mesh.get_instance_id()
	if not _shapes.has(key):
		_shapes[key] = mesh.create_convex_shape(true, true)
	return _shapes[key]


func _set_root_collision(root: Node, enabled: bool) -> void:
	for node in root.find_children("*", "StaticBody3D", true, false):
		node.collision_layer = 1 if enabled else 0


func _hide_baked_copies() -> void:
	var world := get_parent().get_node_or_null("World")
	if world == null:
		return
	var names: Array = manifest.get("static_duplicates", []).duplicate()
	for item: Dictionary in manifest.get("loose", []):
		names.append(item.source_node)
	for node_name: String in names:
		var old := world.get_node_or_null(NodePath(node_name))
		if old != null:
			old.queue_free()


func _model(model: String) -> Node3D:
	var path: String = manifest.get("models", {}).get(model, "")
	if path.is_empty():
		return null
	if not _models.has(model):
		_models[model] = load(path)
	return (_models[model] as PackedScene).instantiate() as Node3D


static func _meshes(root: Node) -> Array:
	var result: Array = root.find_children("*", "MeshInstance3D", true, false)
	if root is MeshInstance3D:
		result.push_front(root)
	return result


static func _entity_basis(angles: Vector3) -> Basis:
	return (
		Basis(Vector3.UP, deg_to_rad(angles.y))
		* Basis(Vector3.BACK, -deg_to_rad(angles.x))
		* Basis(Vector3.RIGHT, deg_to_rad(angles.z))
	)


static func _vec(values: Array) -> Vector3:
	return Vector3(float(values[0]), float(values[1]), float(values[2]))


static func _read_json(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		push_error("Missing Nuketown prop data: " + path)
		return {}
	var data = JSON.parse_string(file.get_as_text())
	return data if data is Dictionary else {}


func _setup_animation(instance: Node3D, model: String, entity_id: int) -> void:
	if not behavior.get("models", {}).has(model):
		return
	# Car-flip clips belong to the two original nuke-outro cars only.
	if model.begins_with("veh_") and entity_id not in [58, 59]:
		return
	var player := ANIMATION.new()
	player.configure(instance, model, behavior)
	player.set_visibility_enabled(not model.begins_with("veh_"))
	var clips: Array = behavior.get("clips_by_model", {}).get(model, [])
	if clips.is_empty():
		return
	var clip := String(clips[0])
	if model.contains("carwash"):
		clip = "fxanim_mp_nuked2025_carwash_hoses_anim"
	var looping := not (
		model.begins_with("veh_") or model.contains("display_glass") or model.contains("dome")
	)
	player.play(clip, looping)
	if model.begins_with("veh_"):
		for record: Dictionary in pieces.values():
			if record.get("entity", -2) == entity_id:
				_show_stage(record, record.stage)
	_animated.append({"player": player, "clip": clip, "loop": looping, "root": instance})


func play_outro() -> void:
	_ended = true
	for entry: Dictionary in _animated:
		if not entry.loop:
			entry.player.play(entry.clip, false)
	# The source outro swaps the live cars for their pristine animated copies.
	for record: Dictionary in pieces.values():
		if record.kind == "destructible" and record.car:
			if record.entity in [58, 59]:
				_show_stage(record, 0)
			else:
				record.root.visible = false


static func _source_vector(text: String) -> Vector3:
	var parts := text.split(" ", false)
	return Vector3(float(parts[0]), float(parts[2]), -float(parts[1])) * 0.0254


func _trimesh(mesh: Mesh) -> Shape3D:
	var key := "tri_%d" % mesh.get_instance_id()
	if not _shapes.has(key):
		var shape := mesh.create_trimesh_shape()
		shape.backface_collision = true
		_shapes[key] = shape
	return _shapes[key]
