extends SceneTree
## CPU benchmark: where a frame's CPU time goes on the client, and a tick's on
## the server, in a real match. Starts a dedicated server with bots in a
## background process (profiled through `SHOOTER_CPU_PROFILE`), joins it,
## plays for a while standing at the spawn, and prints each system's time per
## frame (client) and per tick (server). Needs a GPU and a display (or Xvfb):
##   tools/cpu_bench.sh [--map slamcity] [--mode tdm] [--bots 11] [--seconds 20]
## or directly:
##   godot --path godot --resolution 1920x1080 --audio-driver Dummy \
##     --script res://tests/cpu_bench.gd -- --map slamcity --bots 11 --out cpu.json
##
## Options (after `--`):
##   --map      a map id as `Net` takes it (default slamcity).
##   --mode     tdm, dom, ... (default tdm).
##   --bots     bots on the server (default 11: a 6v6 with this client).
##   --seconds  seconds measured once the match is playing (default 20).
##   --warmup   seconds played before measuring (default 5).
##   --out      JSON results file (optional; a table always prints).
##
## Client systems are the Rust scopes (`CpuProfile`, rust/game/src/profile.rs)
## plus three totals timed here: the process step (from the tree's
## `process_frame` to the renderer's `frame_pre_draw`: every `_process`,
## GDScript and Rust, animation trees, and deferred calls), the physics step
## (from `physics_frame` to `process_frame`: `_physics_process` and the physics
## engine), and the renderer's own CPU time. Godot's TIME_PROCESS monitor is
## not used: it includes drawing and waiting on the display.

## Rust systems that run in the physics step, the rest run in the process
## step (see `profile::scope` calls in rust/game/src). Scopes under "body/"
## and "bots/" run inside others and are not summed.
const PHYSICS_SYSTEMS := [
	"client/net",
	"player (physics)",
	"ragdolls",
	"sentry",
]

var _options := {
	"map": "slamcity",
	"mode": "tdm",
	"bots": "11",
	"seconds": "20",
	"warmup": "5",
	"out": "",
}
var _port := 32000 + OS.get_process_id() % 20000
var _server_pid := -1
var _profile_path := ""

## Process time per subtree: label to total ms (see `_insert_markers`).
var _subtree_ms := {}
var _marks: Array = []
var _marking := false
var _last_process_ms := 0.0
var _last_physics_ms := 0.0
var _process_start := 0
var _physics_start := 0


func _initialize() -> void:
	physics_frame.connect(_on_physics_frame)
	process_frame.connect(_on_process_frame)
	RenderingServer.frame_pre_draw.connect(_on_pre_draw)
	_run.call_deferred()


## Timestamps for the steps: physics ticks (none, one, or several) run before
## each frame's process step, which ends where drawing begins.
func _on_physics_frame() -> void:
	if _physics_start == 0:
		_physics_start = Time.get_ticks_usec()


func _on_process_frame() -> void:
	if _marking:
		CpuProfile.end_frame()
	_process_start = Time.get_ticks_usec()
	_last_physics_ms = 0.0
	if _physics_start != 0:
		_last_physics_ms = (_process_start - _physics_start) / 1000.0
		_physics_start = 0


func _on_pre_draw() -> void:
	var now := Time.get_ticks_usec()
	if _process_start != 0:
		_last_process_ms = (now - _process_start) / 1000.0
	if _marking and not _marks.is_empty():
		var at := _process_start
		var label := "(before the first marker)"
		for mark: Array in _marks:
			_subtree_ms[label] = _subtree_ms.get(label, 0.0) + (mark[1] - at) / 1000.0
			label = mark[0]
			at = mark[1]
		_subtree_ms[label] = _subtree_ms.get(label, 0.0) + (now - at) / 1000.0
	_marks.clear()


func _mark(label: String) -> void:
	_marks.append([label, Time.get_ticks_usec()])


## Puts a marker before each child of the root (the autoloads and the map),
## of the map, and of each of the map's canvas layers (the HUD). Within a
## process priority, Godot processes nodes in tree order, so the time from one
## marker to the next is what the subtree after it cost: its scripts, Rust
## nodes, and animation trees.
func _insert_markers() -> void:
	var parents: Array[Node] = [root, current_scene]
	for node in current_scene.find_children("*", "CanvasLayer", true, false):
		parents.append(node)
	for parent in parents:
		var children := parent.get_children()
		for child in children:
			if child is Marker:
				continue
			var marker := Marker.new()
			marker.bench = self
			marker.label = str(root.get_path_to(child)).trim_prefix(str(current_scene.name) + "/")
			parent.add_child(marker)
			parent.move_child(marker, child.get_index())
	var last := Marker.new()
	last.bench = self
	last.label = "(nodes with a higher process priority)"
	root.add_child(last)
	# Runs after every node: what follows is deferred calls and skeleton
	# updates (the message queue), until drawing begins.
	var after := Marker.new()
	after.bench = self
	after.label = "(deferred calls and skeleton updates)"
	after.process_priority = 1 << 30
	root.add_child(after)


func _subtree_rows(frames: int) -> Dictionary:
	var rows := {}
	for label: String in _subtree_ms:
		rows[label] = _subtree_ms[label] / maxf(frames, 1.0)
	return rows


func _print_subtrees(frames: int) -> void:
	var rows := _subtree_rows(frames)
	var labels := rows.keys().filter(func(l): return rows[l] >= 0.02)
	labels.sort_custom(func(a, b): return rows[a] > rows[b])
	print("\nProcess step by subtree, ms per frame (0.02 ms and up)")
	for label: String in labels:
		print("  %-60s %6.3f" % [label.left(60), rows[label]])


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		var key := args[i].trim_prefix("--")
		if _options.has(key) and i + 1 < args.size():
			_options[key] = args[i + 1]
	var settings: Node = root.get_node("Settings")
	settings.path = "user://cpu_bench_settings.cfg"
	settings.set_setting(&"vsync", false)
	settings.set_setting(&"max_fps", 0)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	settings.set_graphics_preset(1)
	RenderingServer.viewport_set_measure_render_time(root.get_viewport_rid(), true)

	var map := String(_options.map)
	_profile_path = OS.get_user_data_dir().path_join("cpu-bench-server-%d.json" % _port)
	DirAccess.remove_absolute(_profile_path)
	OS.set_environment("SHOOTER_CPU_PROFILE", _profile_path)
	var server_args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(_port),
			"--query-port",
			"-1",
			"--map",
			map,
			"--mode",
			String(_options.mode),
			"--bots",
			String(_options.bots),
			"--bot-skill",
			"normal",
			"--score-limit",
			"0",
			"--time-limit",
			"30",
			"--auto-start",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), server_args)
	OS.unset_environment("SHOOTER_CPU_PROFILE")
	var net := root.get_node("Net")
	net.join("127.0.0.1:%d" % _port, {"map": map, "name": "CPU bench"})
	var client: NetClient = null
	for i in 3000:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase == "playing":
			break
		await process_frame
	if client == null or not client.is_joined():
		push_error("cpu bench: could not join the server")
		_finish(1)
		return
	var hud := current_scene.get_node_or_null("TdmHud")
	if hud != null and hud.has_method("_set_menu_open"):
		hud.call("_set_menu_open", false)
	await _seconds(float(_options.warmup))

	_insert_markers()
	await _seconds(0.5)
	_subtree_ms.clear()
	_marking = true
	CpuProfile.set_enabled(true)
	CpuProfile.reset()
	var server_before := _read_server()
	var frames := 0
	var process_ms := 0.0
	var physics_ms := 0.0
	var render_ms := 0.0
	var gpu_ms := 0.0
	var process_frames := PackedFloat64Array()
	var physics_frames := PackedFloat64Array()
	var start := Time.get_ticks_usec()
	var end := start + int(float(_options.seconds) * 1e6)
	var vp := root.get_viewport_rid()
	while Time.get_ticks_usec() < end:
		await process_frame
		frames += 1
		process_ms += _last_process_ms
		physics_ms += _last_physics_ms
		process_frames.append(_last_process_ms)
		physics_frames.append(_last_physics_ms)
		render_ms += RenderingServer.viewport_get_measured_render_time_cpu(vp)
		gpu_ms += RenderingServer.viewport_get_measured_render_time_gpu(vp)
	var wall_s := (Time.get_ticks_usec() - start) / 1e6
	var systems: Dictionary = CpuProfile.report()
	CpuProfile.set_enabled(false)
	_marking = false
	# Wait for the server's next write (every two seconds).
	await _seconds(2.2)
	var server_after := _read_server()

	var client_rows := {}
	var rust_in_process := 0.0
	for name: String in systems:
		var per_frame: float = float(systems[name].total_ms) / frames
		client_rows[name] = {
			"ms": per_frame,
			"median_ms": float(systems[name].median_ms),
			"p95_ms": float(systems[name].p95_ms),
			"max_ms": float(systems[name].max_ms),
			"calls_per_frame": float(systems[name].calls) / frames,
			"slow_calls": int(systems[name].slow_calls),
		}
		var nested := name.begins_with("body/") or name.begins_with("bots/")
		if not name in PHYSICS_SYSTEMS and not nested and not name.begins_with("server"):
			rust_in_process += per_frame
	var totals := {
		"process step, median": _median(process_frames),
		"physics step, median": _median(physics_frames),
		"process step": process_ms / frames,
		"physics step (per frame)": physics_ms / frames,
		"render (CPU)": render_ms / frames,
		"GDScript, animation, engine (process step less Rust)":
		maxf(process_ms / frames - rust_in_process, 0.0),
	}
	var server_rows := _server_rows(server_before, server_after)
	_print_table(frames, wall_s, gpu_ms / frames, totals, client_rows, server_rows)
	_print_subtrees(frames)
	var out := String(_options.out)
	if not out.is_empty():
		var file := FileAccess.open(out, FileAccess.WRITE)
		(
			file
			. store_string(
				(
					JSON
					. stringify(
						{
							"map": map,
							"mode": _options.mode,
							"bots": int(_options.bots),
							"frames": frames,
							"seconds": wall_s,
							"gpu_ms": gpu_ms / frames,
							"client_totals": totals,
							"client_systems": client_rows,
							"server": server_rows,
							"subtrees": _subtree_rows(frames),
						},
						"  "
					)
				)
			)
		)
		print("wrote ", out)
	_finish(0)


func _median(values: PackedFloat64Array) -> float:
	if values.is_empty():
		return 0.0
	var sorted := values.duplicate()
	sorted.sort()
	return sorted[sorted.size() / 2]


func _read_server() -> Dictionary:
	if not FileAccess.file_exists(_profile_path):
		return {}
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(_profile_path))
	return parsed if parsed is Dictionary else {}


## Each server system's time per tick over the measured window (the
## difference of two of the server's reports).
func _server_rows(before: Dictionary, after: Dictionary) -> Dictionary:
	if after.is_empty():
		return {}
	var ticks := float(after.ticks) - float(before.get("ticks", 0))
	if ticks <= 0:
		return {}
	var rows := {
		"_ticks": ticks,
		"_players": after.players,
		"_bots": after.bots,
		"_tick_budget_ms": 1000.0 / float(after.tick_rate),
	}
	var old: Dictionary = before.get("systems", {})
	for name: String in after.systems:
		var now: Dictionary = after.systems[name]
		var was: float = float(old[name].total_ms) if old.has(name) else 0.0
		rows[name] = {
			"ms": (float(now.total_ms) - was) / ticks,
			"median_ms": float(now.get("median_ms", 0.0)),
			"p95_ms": float(now.get("p95_ms", 0.0)),
			"max_ms": float(now.max_ms),
		}
	return rows


func _print_table(
	frames: int,
	wall_s: float,
	gpu: float,
	totals: Dictionary,
	client: Dictionary,
	server: Dictionary,
) -> void:
	print(
		(
			"\nCPU bench: %s %s, %s bots, %d frames in %.1f s (%.0f fps), GPU %.2f ms"
			% [_options.map, _options.mode, _options.bots, frames, wall_s, frames / wall_s, gpu]
		)
	)
	print("\nClient, ms per frame (mean, then median and 95th percentile over frames)")
	for name: String in totals:
		print("  %-44s %6.3f" % [name, totals[name]])
	var names := client.keys()
	names.sort_custom(func(a, b): return client[a].ms > client[b].ms)
	for name: String in names:
		print(
			(
				"  %-44s %6.3f  median %6.3f  p95 %6.3f  (%.1f calls, longest %.2f)"
				% [
					name,
					client[name].ms,
					client[name].median_ms,
					client[name].p95_ms,
					client[name].calls_per_frame,
					client[name].max_ms,
				]
			)
		)
	if server.is_empty():
		print("\nServer: no profile (is SHOOTER_CPU_PROFILE written?)")
		return
	print(
		(
			"\nServer, ms per tick (%d players, %d bots; budget %.2f ms)"
			% [server._players, server._bots, server._tick_budget_ms]
		)
	)
	var server_names := server.keys().filter(func(n): return not String(n).begins_with("_"))
	server_names.sort()
	for name: String in server_names:
		print(
			(
				"  %-44s %6.3f  median %6.3f  p95 %6.3f  (longest %.2f)"
				% [
					name,
					server[name].ms,
					server[name].median_ms,
					server[name].p95_ms,
					server[name].max_ms,
				]
			)
		)


func _finish(code: int) -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	DirAccess.remove_absolute(_profile_path)
	quit(code)


func _seconds(seconds: float) -> void:
	var until := Time.get_ticks_usec() + int(seconds * 1e6)
	while Time.get_ticks_usec() < until:
		await process_frame


## Stamps the clock when the tree's process walk reaches it.
class Marker:
	extends Node
	var bench: Object
	var label := ""

	func _process(_delta: float) -> void:
		bench.call(&"_mark", label)
