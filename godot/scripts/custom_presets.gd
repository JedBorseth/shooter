extends RefCounted
## Custom game settings as the menus keep them (docs/custom-games.md): a
## dictionary per set of rules, the text form servers take
## (`shooter_sim::custom::Settings::parse`), the built-in presets, and named
## presets saved in `user://custom_games.cfg` with the last rules used.
##
## Keys: `hardcore`, `health` (percent), `regen`, `ff`, `respawn` (seconds,
## -1 for the mode's), `killcam`, `radar`, `streaks`, `loadouts`.

const PATH := "user://custom_games.cfg"
const SECTION := "presets"
const LAST := "last"
## Most named presets kept.
const MAX_PRESETS := 12

const DEFAULTS := {
	"hardcore": false,
	"health": 100,
	"regen": "normal",
	"ff": "off",
	"respawn": -1,
	"killcam": true,
	"radar": "normal",
	"streaks": true,
	"loadouts": "any",
}

## Built-in presets, in menu order, as changes from the defaults.
const BUILT_IN := [
	["Standard", {}],
	["Hardcore", {"hardcore": true, "health": 30, "ff": "on"}],
	["Pistols only", {"loadouts": "pistols"}],
	["Snipers only", {"loadouts": "snipers"}],
	["No streaks", {"streaks": false}],
]


## The defaults with `changes` laid over them.
static func with(changes: Dictionary) -> Dictionary:
	var settings := DEFAULTS.duplicate()
	for key: String in changes:
		if DEFAULTS.has(key):
			settings[key] = changes[key]
	return settings


## The text form for `--custom`: only what differs from the defaults, with
## hardcore first (it sets health and friendly fire, so they follow it).
static func to_text(settings: Dictionary) -> String:
	var s := with(settings)
	var parts := PackedStringArray()
	var hardcore: bool = s.hardcore
	if hardcore:
		parts.append("hardcore=on")
	if hardcore or int(s.health) != 100:
		parts.append("health=%d" % int(s.health))
	if s.regen != DEFAULTS.regen:
		parts.append("regen=%s" % s.regen)
	if hardcore or s.ff != DEFAULTS.ff:
		parts.append("ff=%s" % s.ff)
	if int(s.respawn) >= 0:
		parts.append("respawn=%d" % int(s.respawn))
	if not s.killcam:
		parts.append("killcam=off")
	if s.radar != DEFAULTS.radar:
		parts.append("radar=%s" % s.radar)
	if not s.streaks:
		parts.append("streaks=off")
	if s.loadouts != DEFAULTS.loadouts:
		parts.append("loadouts=%s" % s.loadouts)
	return ",".join(parts)


## Whether `settings` are the normal rules.
static func is_default(settings: Dictionary) -> bool:
	return to_text(settings) == ""


## A short line naming what differs ("Hardcore · Pistols only"), or "".
static func summary(settings: Dictionary) -> String:
	var s := with(settings)
	var parts := PackedStringArray()
	if s.hardcore:
		parts.append("Hardcore")
	if int(s.health) != (30 if s.hardcore else 100):
		parts.append("%d%% health" % int(s.health))
	if s.regen != "normal":
		parts.append("Regen %s" % s.regen)
	if s.ff != ("on" if s.hardcore else "off"):
		parts.append("Friendly fire %s" % s.ff)
	if int(s.respawn) >= 0:
		parts.append("Respawn %d s" % int(s.respawn))
	if not s.killcam:
		parts.append("No killcam")
	if s.radar != "normal":
		parts.append("Radar %s" % s.radar)
	if not s.streaks:
		parts.append("No streaks")
	match String(s.loadouts):
		"default":
			parts.append("Default classes")
		"pistols":
			parts.append("Pistols only")
		"snipers":
			parts.append("Snipers only")
	return "  ·  ".join(parts)


## The saved presets: name to settings, in saved order.
static func saved() -> Dictionary:
	var file := ConfigFile.new()
	if file.load(PATH) != OK or not file.has_section(SECTION):
		return {}
	var out := {}
	for key: String in file.get_section_keys(SECTION):
		var value: Variant = file.get_value(SECTION, key, {})
		if value is Dictionary:
			out[key] = with(value)
	return out


## Saves `settings` under `preset_name`, replacing one of that name. The
## oldest go once there are more than `MAX_PRESETS`.
static func save(preset_name: String, settings: Dictionary) -> void:
	var clean := preset_name.strip_edges().left(24)
	if clean.is_empty():
		return
	var file := ConfigFile.new()
	file.load(PATH)
	if file.has_section_key(SECTION, clean):
		file.erase_section_key(SECTION, clean)
	file.set_value(SECTION, clean, with(settings))
	var keys := file.get_section_keys(SECTION)
	while keys.size() > MAX_PRESETS:
		file.erase_section_key(SECTION, keys[0])
		keys = file.get_section_keys(SECTION)
	file.save(PATH)


## Removes the saved preset `preset_name`.
static func remove(preset_name: String) -> void:
	var file := ConfigFile.new()
	if file.load(PATH) != OK or not file.has_section_key(SECTION, preset_name):
		return
	file.erase_section_key(SECTION, preset_name)
	file.save(PATH)


## The rules used last, or the defaults.
static func last() -> Dictionary:
	var file := ConfigFile.new()
	if file.load(PATH) != OK:
		return DEFAULTS.duplicate()
	var value: Variant = file.get_value(LAST, "settings", {})
	return with(value if value is Dictionary else {})


static func remember(settings: Dictionary) -> void:
	var file := ConfigFile.new()
	file.load(PATH)
	file.set_value(LAST, "settings", with(settings))
	file.save(PATH)
