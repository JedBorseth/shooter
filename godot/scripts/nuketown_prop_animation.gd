class_name NuketownPropAnimation
extends RefCounted
## Plays one of Nuketown's source rigid-bone xanim clips on a flattened prop.
##
## The generated prop models under ``assets/maps/nuketown/props/models`` are
## skinless: every mesh node is named after the source bone (``bone`` plus
## ``_1``, ``_2`` ... for extra surfaces) and its vertices are already in the
## bind pose, in y-up metres.  This helper rebuilds the source skeleton from
## ``behavior.json``'s per-model bind table, evaluates the clip, and sets each
## mesh node to ``animated bone global * inverse bind global``.  At rest those
## two transforms cancel to identity, which is why the imported vertices need
## no per-vertex skinning.
##
## Data comes from ``assets/maps/nuketown/props/behavior.json``, prepared by
## ``tools/prepare_nuketown_behavior.py``.  Rotations are xyzw quaternions and
## all positions are metres in the model's y-up space.
##
## Clip rules follow Black Ops II's documented xanim behaviour: a non-root
## rotation key replaces the bind rotation, a root rotation key is composed
## with the bind rotation, and translation keys are offsets added to the bind
## translation (rotated by the bind rotation on the root joint).  The clip's
## separate delta/root-motion track is an extra model-space transform; the
## preparation tool has already changed its source z-up displacement and
## quaternion into y-up.

const EPSILON := 1e-6

var _root: Node3D
var _model_name := ""
var _configured := false
var _bone_count := 0
var _bone_names: PackedStringArray = PackedStringArray()
var _parents: PackedInt32Array = PackedInt32Array()
var _bind_translations: Array[Vector3] = []
var _bind_rotations: Array[Quaternion] = []
var _inverse_binds: Array[Transform3D] = []
var _bone_nodes: Array = []
var _bone_index: Dictionary = {}
var _clips: Dictionary = {}
var _clip: Dictionary = {}
var _clip_name := ""
var _duration := 0.0
var _fps := 30.0
var _time := 0.0
var _loop := false
var _playing := false
var _visibility_enabled := true


## Bind this helper to one instantiated generated prop scene.
func configure(model_root: Node3D, model_name: String, behavior: Dictionary) -> void:
	_restore_visibility()
	_root = model_root
	_model_name = model_name
	_configured = false
	_clips.clear()
	_clip = {}
	_clip_name = ""
	_duration = 0.0
	_fps = 30.0
	_time = 0.0
	_loop = false
	_playing = false
	_visibility_enabled = true
	_bone_names = PackedStringArray()
	_parents = PackedInt32Array()
	_bind_translations.clear()
	_bind_rotations.clear()
	_inverse_binds.clear()
	_bone_nodes.clear()
	_bone_index.clear()

	for item in behavior.get("animations", []):
		if item is Dictionary:
			var name := String(item.get("name", ""))
			var clip_models: Array = item.get("models", [])
			if not name.is_empty() and (clip_models.is_empty() or model_name in clip_models):
				_clips[name] = item

	if model_root == null:
		push_error("NuketownPropAnimation: no model root for " + model_name)
		return
	var models: Dictionary = behavior.get("models", {})
	if not models.has(model_name):
		push_error("NuketownPropAnimation: behavior.json has no bind table for " + model_name)
		return
	var model: Dictionary = models[model_name]
	var bones: Array = model.get("bones", [])
	_bone_count = bones.size()
	if _bone_count == 0:
		push_error("NuketownPropAnimation: empty bind table for " + model_name)
		return
	for index in _bone_count:
		var bone: Dictionary = bones[index]
		var name := String(bone.get("name", ""))
		_bone_names.append(name)
		_bone_index[name] = index
		_parents.append(int(bone.get("parent", -1)))
		_bind_translations.append(_to_vec3(bone.get("translation", [0.0, 0.0, 0.0])))
		_bind_rotations.append(_to_quat(bone.get("rotation", [0.0, 0.0, 0.0, 1.0])))
		_inverse_binds.append(_inverse_bind(bone.get("inverse_bind", [])))
		_bone_nodes.append([])

	var meshes: Array = []
	_collect_meshes(_root, meshes)
	var order: Array[int] = []
	for index in _bone_count:
		order.append(index)
	order.sort_custom(
		func(a: int, b: int) -> bool: return _bone_names[a].length() > _bone_names[b].length()
	)
	for mesh: MeshInstance3D in meshes:
		for index in order:
			if _matches_bone(String(mesh.name), _bone_names[index]):
				(_bone_nodes[index] as Array).append(mesh)
				break
	_configured = true
	stop()


## Start a clip at frame zero.  A clip is only looped when both the caller and
## the source clip allow it, so the one-shot dome/car clips never loop by
## accident while the sprinkler, gate signs and hoses can.
func play(clip_name: String, loop: bool = true) -> void:
	if not _clips.has(clip_name):
		push_error("NuketownPropAnimation: unknown clip " + clip_name)
		return
	if not _configured:
		push_error("NuketownPropAnimation: play() before a successful configure()")
		return
	_restore_visibility()
	_clip = _clips[clip_name]
	_clip_name = clip_name
	_duration = maxf(0.0, float(_clip.get("duration_seconds", 0.0)))
	_fps = maxf(EPSILON, float(_clip.get("fps", 30.0)))
	_loop = loop and bool(_clip.get("loop", true))
	_time = 0.0
	_playing = true
	sample(0.0)


## Sample an absolute clip time.  Looping clips wrap at their duration; one-shot
## clips hold their final frame.
func sample(time_seconds: float) -> void:
	if not _configured or _clip.is_empty():
		return
	if _loop and _duration > 0.0:
		_time = fposmod(time_seconds, _duration)
	else:
		_time = clampf(time_seconds, 0.0, _duration)
	if _time >= _duration and not _loop:
		_playing = false
	_apply_pose(_time)
	_apply_visibility(_time * _fps)


## Advance the current clip by a frame delta.
func advance(delta: float) -> void:
	if not _configured or not _playing or _clip.is_empty() or delta <= 0.0:
		return
	sample(_time + delta)


## Whether this model's behavior data contains a playable clip.
func has_clip(name: String) -> bool:
	return _configured and _clips.has(name)


## Return the clip names prepared for this behavior file.
func clip_names() -> PackedStringArray:
	var names := PackedStringArray()
	for name in _clips:
		names.append(String(name))
	return names


## Stop and restore the bind pose.  Only meshes whose visibility this helper
## changed (the display glass) are made visible again; a destructible prop's
## hidden damage pieces are left to the props manager.
func stop() -> void:
	_restore_visibility()
	_clip = {}
	_clip_name = ""
	_duration = 0.0
	_time = 0.0
	_loop = false
	_playing = false
	if not _configured:
		return
	_apply_rest_pose()


func set_visibility_enabled(enabled: bool) -> void:
	if _visibility_enabled and not enabled:
		_restore_visibility()
	_visibility_enabled = enabled


func playing_clip() -> String:
	return _clip_name


func clip_time() -> float:
	return _time


func clip_duration() -> float:
	return _duration


func is_playing() -> bool:
	return _playing


func is_looping() -> bool:
	return _loop


func configured_model() -> String:
	return _model_name


func _apply_pose(time_seconds: float) -> void:
	var locals: Array[Transform3D] = []
	locals.resize(_bone_count)
	var tracks: Dictionary = _clip.get("tracks", {})
	for index in _bone_count:
		var basis := Basis(_bind_rotations[index])
		var origin := _bind_translations[index]
		var track: Dictionary = tracks.get(_bone_names[index], {})
		if track.has("rotation"):
			var keyed := _sample_quat(track["rotation"], time_seconds)
			if _parents[index] < 0:
				basis = Basis(_bind_rotations[index] * keyed)
			else:
				basis = Basis(keyed)
		if track.has("translation"):
			var offset := _sample_vec(track["translation"], time_seconds)
			if _parents[index] < 0:
				offset = _bind_rotations[index] * offset
			origin = _bind_translations[index] + offset
		locals[index] = Transform3D(basis, origin)

	var globals: Array[Transform3D] = []
	globals.resize(_bone_count)
	for index in _bone_count:
		var parent := _parents[index]
		if parent < 0:
			globals[index] = locals[index]
		else:
			globals[index] = globals[parent] * locals[index]

	var root_motion := _sample_root_motion(time_seconds)
	for index in _bone_count:
		var pose := root_motion * globals[index] * _inverse_binds[index]
		for node: Node3D in _bone_nodes[index]:
			if is_instance_valid(node):
				_set_node_pose(node, pose)


func _apply_rest_pose() -> void:
	for index in _bone_count:
		for node: Node3D in _bone_nodes[index]:
			if is_instance_valid(node):
				_set_node_pose(node, Transform3D.IDENTITY)


## Set a mesh node's local transform so its model-root-space transform is pose.
## Generated models are flat (all mesh nodes are direct children), but this also
## handles an importer that wraps them in extra Node3D transforms.
func _set_node_pose(node: Node3D, pose: Transform3D) -> void:
	if node == _root:
		return
	var parent := node.get_parent()
	if parent == _root:
		node.transform = pose
		return
	var parent_transform := Transform3D.IDENTITY
	var chain: Node = parent
	while chain != null and chain != _root:
		if chain is Node3D:
			parent_transform = (chain as Node3D).transform * parent_transform
		chain = chain.get_parent()
	node.transform = parent_transform.affine_inverse() * pose


func _sample_root_motion(time_seconds: float) -> Transform3D:
	if not _clip.has("root_motion"):
		return Transform3D.IDENTITY
	var motion: Dictionary = _clip["root_motion"]
	var rotation := Quaternion.IDENTITY
	var translation := Vector3.ZERO
	if motion.has("rotation"):
		rotation = _sample_quat(motion["rotation"], time_seconds)
	if motion.has("translation"):
		translation = _sample_vec(motion["translation"], time_seconds)
	return Transform3D(Basis(rotation), translation)


func _sample_quat(track: Dictionary, time_seconds: float) -> Quaternion:
	var segment := _segment(track, time_seconds)
	if segment.is_empty():
		return Quaternion.IDENTITY
	var first := _to_quat(segment[0])
	if segment[2] <= 0.0:
		return first
	var second := _to_quat(segment[1])
	if first.dot(second) < 0.0:
		second = -second
	return first.slerp(second, float(segment[2])).normalized()


func _sample_vec(track: Dictionary, time_seconds: float) -> Vector3:
	var segment := _segment(track, time_seconds)
	if segment.is_empty():
		return Vector3.ZERO
	var first := _to_vec3(segment[0])
	if segment[2] <= 0.0:
		return first
	return first.lerp(_to_vec3(segment[1]), float(segment[2]))


## [first value, second value, blend] for the key pair around the clip time.
func _segment(track: Dictionary, time_seconds: float) -> Array:
	var indices: Array = track.get("indices", [])
	var values: Array = track.get("values", [])
	if values.is_empty():
		return []
	if values.size() == 1 or indices.size() < 2:
		return [values[0], values[0], 0.0]
	var frame := time_seconds * _fps
	var first_frame := float(indices[0])
	var last_index := mini(indices.size(), values.size()) - 1
	var last_frame := float(indices[last_index])
	if frame <= first_frame:
		return [values[0], values[0], 0.0]
	if frame >= last_frame:
		return [values[last_index], values[last_index], 0.0]
	for index in last_index:
		var start := float(indices[index])
		var end := float(indices[index + 1])
		if frame <= end:
			var span := maxf(end - start, EPSILON)
			return [values[index], values[index + 1], clampf((frame - start) / span, 0.0, 1.0)]
	return [values[last_index], values[last_index], 0.0]


func _apply_visibility(frame: float) -> void:
	if not _visibility_enabled:
		return
	var rules: Dictionary = _clip.get("visibility", {})
	if rules.is_empty():
		return
	var shatter_frame := float(rules.get("shatter_frame", 0))
	var phase: Dictionary = rules.get("before" if frame < shatter_frame else "after", {})
	for bone in phase.get("show", []):
		_set_bone_visible(String(bone), true)
	for bone in phase.get("hide", []):
		_set_bone_visible(String(bone), false)


func _set_bone_visible(bone: String, visible: bool) -> void:
	if not _bone_index.has(bone):
		return
	for node: Node3D in _bone_nodes[_bone_index[bone]]:
		if is_instance_valid(node):
			node.visible = visible


func _restore_visibility() -> void:
	var rules: Dictionary = _clip.get("visibility", {})
	if rules.is_empty():
		return
	for phase_name in ["before", "after"]:
		var phase: Dictionary = rules.get(phase_name, {})
		for bone in phase.get("show", []):
			_set_bone_visible(String(bone), true)
		for bone in phase.get("hide", []):
			_set_bone_visible(String(bone), true)


static func _collect_meshes(node: Node, result: Array) -> void:
	if node is MeshInstance3D:
		result.append(node)
	for child in node.get_children():
		_collect_meshes(child, result)


static func _matches_bone(node_name: String, bone: String) -> bool:
	if node_name == bone:
		return true
	var prefix := bone + "_"
	if not node_name.begins_with(prefix):
		return false
	return node_name.substr(prefix.length()).is_valid_int()


static func _to_vec3(value: Array) -> Vector3:
	if value.size() < 3:
		return Vector3.ZERO
	return Vector3(float(value[0]), float(value[1]), float(value[2]))


static func _to_quat(value: Array) -> Quaternion:
	if value.size() < 4:
		return Quaternion.IDENTITY
	return (
		Quaternion(float(value[0]), float(value[1]), float(value[2]), float(value[3])).normalized()
	)


## [x axis, y axis, z axis, origin] as exported by prepare_nuketown_behavior.py.
static func _inverse_bind(value: Array) -> Transform3D:
	if value.size() < 4:
		return Transform3D.IDENTITY
	return Transform3D(
		_to_vec3(value[0]), _to_vec3(value[1]), _to_vec3(value[2]), _to_vec3(value[3])
	)
