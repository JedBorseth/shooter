extends Node
## Small, periodic breadcrumbs; no player names, chat, input contents, or credentials.

var _elapsed := 0.0


func _ready() -> void:
	process_mode = Node.PROCESS_MODE_ALWAYS
	print("diagnostics: start UTC=", Time.get_datetime_string_from_system(true))
	print("diagnostics: engine=", Engine.get_version_info().string, " OS=", OS.get_name())
	print(
		"diagnostics: renderer=",
		RenderingServer.get_current_rendering_method(),
		" GPU=",
		RenderingServer.get_video_adapter_name(),
		" API=",
		RenderingServer.get_video_adapter_api_version()
	)
	print("diagnostics: engine log=", OS.get_user_data_dir().path_join("logs/godot.log"))


func _process(delta: float) -> void:
	_elapsed += delta
	if _elapsed < 5.0:
		return
	_elapsed = 0.0
	var scene := get_tree().current_scene
	print(
		"diagnostics: uptime_ms=",
		Time.get_ticks_msec(),
		" scene=",
		scene.scene_file_path if scene != null else "transition",
		" fps=",
		Engine.get_frames_per_second(),
		" static_memory=",
		OS.get_static_memory_usage(),
		" video_memory=",
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_VIDEO_MEM_USED)
	)
	var net := get_node_or_null("/root/Net")
	if net != null and net.is_online():
		print("diagnostics: network=", net.client().stats())
