extends SceneTree
## Compiles every GDScript the game, its tests, and its tools ship, and fails
## on any that does not. A script with a parse error loads as nothing: the
## game prints "Failed to load script" and carries on without that HUD piece
## or menu, so a test that never touches it still passes. This test catches
## those. It compiles after the first frame, when the autoloads exist, so
## scripts that name an autoload compile as they do in the game.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/scripts_compile_test.gd

const ROOTS := ["res://scripts", "res://tests", "res://tools"]

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await process_frame
	var paths: PackedStringArray = []
	for dir in ROOTS:
		_collect(dir, paths)
	for path in paths:
		var script := ResourceLoader.load(path, "GDScript", ResourceLoader.CACHE_MODE_REUSE)
		if script == null:
			_failures.append("%s does not load" % path)
		elif not (script as GDScript).can_instantiate():
			_failures.append("%s does not compile" % path)
	print("scripts_compile_test: compiled %d scripts" % paths.size())
	if _failures.is_empty():
		print("scripts_compile_test: OK")
		quit(0)
	else:
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)


func _collect(dir: String, paths: PackedStringArray) -> void:
	for file in DirAccess.get_files_at(dir):
		if file.ends_with(".gd"):
			paths.append(dir.path_join(file))
	for sub in DirAccess.get_directories_at(dir):
		_collect(dir.path_join(sub), paths)
