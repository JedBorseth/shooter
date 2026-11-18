extends SceneTree
## Headless ACOG test. On every rifle that takes the ACOG, fitting it builds
## the eyepiece (its rim, glass, and reticle) centred on the sight line, and
## the housing ahead of it as a separate `SightBody` that first person hides
## when fully aimed. Taking it off removes both.
## godot --headless --path godot --script res://tests/acog_sight_test.gd

const WEAPONS := ["m4", "ak47", "lsw6", "pdw9", "br3"]

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for key: String in WEAPONS:
		await _check_acog(key)
	print("ACOG sight test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_acog(key: String) -> void:
	var view := (
		(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
	)
	root.add_child(view)
	await process_frame
	view.call("apply_attachment", "acog")
	await process_frame
	var eyepiece := view.get_node_or_null("Attachment/Optic/SightMesh") as MeshInstance3D
	var body := view.get_node_or_null("Attachment/Optic/SightBody") as MeshInstance3D
	_check(eyepiece != null, "%s: the ACOG has an eyepiece" % key)
	_check(body != null, "%s: the ACOG's body is its own mesh" % key)
	if eyepiece != null and body != null:
		var near := _bounds_in(eyepiece, view)
		var ahead := _bounds_in(body, view)
		var centre := near.get_center()
		_check(
			absf(centre.x) < 0.001 and absf(centre.y) < 0.001,
			(
				"%s: the eyepiece is centred on the sight line (%.1f, %.1f mm)"
				% [key, centre.x * 1000.0, centre.y * 1000.0]
			)
		)
		_check(
			ahead.position.z < near.position.z and ahead.end.z <= near.end.z,
			"%s: the body is ahead of the eyepiece's rim" % key
		)
	view.call("apply_attachment", "")
	await process_frame
	_check(
		view.get_node_or_null("Attachment/Optic") == null,
		"%s: taking the ACOG off removes it" % key
	)
	view.queue_free()
	await process_frame


func _bounds_in(mesh: MeshInstance3D, space: Node3D) -> AABB:
	return space.global_transform.affine_inverse() * mesh.global_transform * mesh.get_aabb()


func _check(ok: bool, message: String) -> void:
	if ok:
		print("ok   ", message)
	else:
		_failures += 1
		push_error("FAIL: " + message)
