extends SceneTree
## Run after the first import, then run --editor --headless --import again.
## -- --size-limit=256 selects a smaller texture budget on a shared server GPU.

const GENERATED := "res://assets/maps/nuketown/generated/"
const DIRECTORY := GENERATED + "textures/"


func _initialize() -> void:
	var size_limit := 1024
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--size-limit="):
			size_limit = int(arg.get_slice("=", 1))
	var kinds := _texture_kinds()
	var count := 0
	var normal_count := 0
	for name in DirAccess.get_files_at(DIRECTORY):
		if not name.ends_with(".png.import"):
			continue
		var config := ConfigFile.new()
		if config.load(DIRECTORY + name) != OK:
			quit(1)
			return
		var kind: String = kinds.get(name.trim_suffix(".import"), "color")
		config.set_value("params", "compress/mode", 2)
		config.set_value("params", "compress/normal_map", kind == "normal")
		config.set_value("params", "mipmaps/generate", true)
		config.set_value("params", "process/size_limit", size_limit)
		if config.save(DIRECTORY + name) != OK:
			quit(1)
			return
		count += 1
		if kind == "normal":
			normal_count += 1
	print(
		(
			"Configured %d Nuketown textures (%d normal maps, size limit %d)"
			% [count, normal_count, size_limit]
		)
	)
	quit(0 if count > 0 else 1)


## The importer writes which generated images are normal maps, so they use
## normal-map-aware VRAM compression instead of colour-oriented S3TC.
func _texture_kinds() -> Dictionary:
	var path := GENERATED + "textures.json"
	if not FileAccess.file_exists(path):
		return {}
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {}
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if parsed is Dictionary:
		return parsed.get("kinds", {})
	return {}
