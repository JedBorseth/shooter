extends SceneTree
## Run after the first import, then run --editor --headless --import again.
## -- --size-limit=256 selects a smaller texture budget on a shared server GPU.


func _initialize() -> void:
	var size_limit := 1024
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--size-limit="):
			size_limit = int(arg.get_slice("=", 1))
	var directory := "res://assets/maps/nuketown/generated/textures/"
	var count := 0
	for name in DirAccess.get_files_at(directory):
		if not name.ends_with(".png.import"):
			continue
		var config := ConfigFile.new()
		if config.load(directory + name) != OK:
			quit(1)
			return
		config.set_value("params", "compress/mode", 2)
		config.set_value("params", "mipmaps/generate", true)
		config.set_value("params", "process/size_limit", size_limit)
		if config.save(directory + name) != OK:
			quit(1)
			return
		count += 1
	print("Configured %d VRAM-compressed Nuketown textures (size limit %d)" % [count, size_limit])
	quit(0 if count > 0 else 1)
