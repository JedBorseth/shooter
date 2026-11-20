extends SceneTree
## Bot match bench: plays whole bot-only matches on a dedicated server in this
## process and writes what happened to JSON, so changes to the bots can be
## compared by outcome instead of by feel. For each map, mode, and match it
## records the winner, the team scores, every bot's kills, deaths, score, and
## captures, the bots' tactics (lanes, cover, grenades), and the objective's
## own log: Search and Destroy rounds (winner, how they ended, the site
## planted, whether the attackers switched sites) and Hardpoint zones (how
## soon each team reached each zone, and how long each held it). Domination
## also samples flag ownership every second.
##
## Run it at a fixed frame rate so it plays faster than real time:
##   godot --headless --fixed-fps 120 --path godot --script res://tools/bot_bench.gd -- \
##     --modes tdm,dom,hardpoint,ctf,snd --maps slamcity,warehouse --skill normal \
##     --bots 12 --matches 2 --minutes 5 --out ../bench.json
## Each match runs to its end or for --minutes of play, whichever comes first.
## --seed changes the bots' random choices; each match adds its index to it.
## The summary printed at the end is also in the JSON under "summary".

const SERVER_SCENE := "res://scenes/server.tscn"
const MAPS := {
	"slamcity": ["res://scenes/slamcity.tscn", "Slamcity"],
	"warehouse": ["res://scenes/warehouse.tscn", "Warehouse"],
	"arena": ["res://scenes/main.tscn", "Arena"],
}

var _options := {
	"modes": "tdm,dom,hardpoint,ctf,snd",
	"maps": "slamcity",
	"skill": "normal",
	"bots": "12",
	"matches": "1",
	"minutes": "5",
	"seed": "1",
	"out": "",
}
var _port := 31000 + OS.get_process_id() % 20000


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		var key := args[i].trim_prefix("--")
		if _options.has(key) and i + 1 < args.size():
			_options[key] = args[i + 1]
	var results := []
	var index := 0
	for map_key in String(_options.maps).split(","):
		if not MAPS.has(map_key):
			push_error("bot bench: unknown map %s" % map_key)
			continue
		for mode in String(_options.modes).split(","):
			for m in int(_options.matches):
				var result: Dictionary = await _play(map_key, mode, int(_options.seed) + m, index)
				results.append(result)
				index += 1
	var summary := _summarize(results)
	var report := {"options": _options, "matches": results, "summary": summary}
	var out := String(_options.out)
	if out != "":
		var file := FileAccess.open(out, FileAccess.WRITE)
		if file:
			file.store_string(JSON.stringify(report, "  "))
			print("bot bench: wrote %s" % out)
		else:
			push_error("bot bench: cannot write %s" % out)
	for line in summary:
		print("bot bench: %s" % line)
	quit(0)


func _play(map_key: String, mode: String, bot_seed: int, index: int) -> Dictionary:
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(MAPS[map_key][0]) as PackedScene
	server.map_name = MAPS[map_key][1]
	server.mode = mode
	server.port = _port + index * 2
	server.query_port = -1
	server.bots = int(_options.bots)
	server.bot_skill = String(_options.skill)
	server.bot_seed = bot_seed
	server.auto_start = true
	root.add_child(server)
	var tps := Engine.physics_ticks_per_second
	for i in 30 * tps:
		await physics_frame
		if server.match_phase() == "playing":
			break
	var started := Time.get_ticks_msec()
	var limit := int(float(_options.minutes) * 60.0 * tps)
	var ticks := 0
	var flag_seconds := [0, 0]
	while ticks < limit and server.match_phase() != "ended":
		await physics_frame
		ticks += 1
		if mode == "dom" and ticks % tps == 0:
			for f in server.dom_flag_count():
				var owner := server.dom_flag_owner(f)
				if owner >= 0 and owner < 2:
					flag_seconds[owner] += 1
	var players := []
	for id in server.player_ids():
		var stats: Dictionary = server.player_stats(id)
		stats["id"] = id
		players.append(stats)
	var result := {
		"map": map_key,
		"mode": mode,
		"seed": bot_seed,
		"seconds": float(ticks) / tps,
		"wall_seconds": (Time.get_ticks_msec() - started) / 1000.0,
		"ended": server.match_phase() == "ended",
		"winner": server.match_winner(),
		"scores": [server.team_score(0), server.team_score(1)],
		"players": players,
		"tactics": server.bot_tactics(),
		"log": server.match_log(),
	}
	if mode == "dom":
		result["flag_seconds"] = flag_seconds
	print(
		(
			"bot bench: %s %s seed %d: %s in %.0f s of play (%.0f s wall), scores %s"
			% [
				map_key,
				mode,
				bot_seed,
				"ended" if result.ended else "cut off",
				result.seconds,
				result.wall_seconds,
				result.scores
			]
		)
	)
	server.queue_free()
	for i in 10:
		await process_frame
	return result


## One line per map and mode: score margin, kill/death spread, and the
## objective's numbers.
func _summarize(results: Array) -> PackedStringArray:
	var groups := {}
	for r in results:
		var key := "%s %s" % [r.map, r.mode]
		if not groups.has(key):
			groups[key] = []
		groups[key].append(r)
	var lines := PackedStringArray()
	for key in groups:
		var group: Array = groups[key]
		var margin := 0.0
		var kills := []
		var ratios := []
		var captures := 0
		var tactics := {}
		for r in group:
			margin += absf(float(r.scores[0]) - float(r.scores[1]))
			for p in r.players:
				kills.append(float(p.kills))
				ratios.append(float(p.kills) / maxf(float(p.deaths), 1.0))
				captures += int(p.captures)
			for t in r.tactics:
				tactics[t] = int(tactics.get(t, 0)) + int(r.tactics[t])
		var line := (
			(
				"%s: %d match(es), score margin %.1f, kills mean %.1f sd %.1f (min %d, max %d),"
				+ " K/D sd %.2f, captures %d"
			)
			% [
				key,
				group.size(),
				margin / group.size(),
				_mean(kills),
				_sd(kills),
				int(kills.min()) if kills.size() else 0,
				int(kills.max()) if kills.size() else 0,
				_sd(ratios),
				captures
			]
		)
		line += _objective_line(group)
		line += ", tactics %s" % [tactics]
		lines.append(line)
	return lines


func _objective_line(group: Array) -> String:
	var rounds := 0
	var reasons := {}
	var plants := 0
	var switched := 0
	var attack_wins := 0
	var arrivals := []
	var flag_seconds := [0, 0]
	for r in group:
		var objective: Dictionary = r.log
		for rnd in objective.rounds:
			rounds += 1
			reasons[rnd.reason] = int(reasons.get(rnd.reason, 0)) + 1
			plants += 1 if int(rnd.planted) >= 0 else 0
			switched += 1 if rnd.switched else 0
			attack_wins += 1 if int(rnd.winner) == int(rnd.attackers) else 0
		for zone in objective.zones:
			for t in zone.first_in:
				if float(t) >= 0.0:
					arrivals.append(float(t))
		if r.has("flag_seconds"):
			flag_seconds[0] += r.flag_seconds[0]
			flag_seconds[1] += r.flag_seconds[1]
	var line := ""
	if rounds > 0:
		line += (
			", %d rounds (attackers won %d, %d plants, %d site switches) %s"
			% [rounds, attack_wins, plants, switched, reasons]
		)
	if arrivals.size() > 0:
		line += ", zone arrival mean %.1f s" % _mean(arrivals)
	if flag_seconds[0] + flag_seconds[1] > 0:
		line += ", flag-seconds owned %s" % [flag_seconds]
	return line


func _mean(values: Array) -> float:
	if values.is_empty():
		return 0.0
	var total := 0.0
	for v in values:
		total += float(v)
	return total / values.size()


func _sd(values: Array) -> float:
	if values.size() < 2:
		return 0.0
	var mean := _mean(values)
	var total := 0.0
	for v in values:
		total += (float(v) - mean) * (float(v) - mean)
	return sqrt(total / (values.size() - 1))
