extends RefCounted
## What the match banner says before and after a match (`match_banner.gd`),
## from `NetClient.match_info`: the countdown's mode and objective, and the
## result (VICTORY on green, DEFEAT on crimson, DRAW on grey), why the match
## ended, and the team strip. `tdm_hud.gd` calls `fill` each frame.
##
## Load it with `preload`; everything here is static.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## What to do in each mode, under the countdown ("%d" is the score limit).
const OBJECTIVES := {
	"ffa": "First to %d kills wins",
	"ctf": "Take the enemy flag to your base  ·  first to %d",
	"hardpoint": "Hold the zone to score  ·  first to %d",
	"dom": "Capture and hold the flags  ·  first to %d",
	"snd": "Plant or defuse the charge  ·  first to %d rounds",
	"tdm": "Eliminate the enemy team  ·  first to %d kills",
}


## Sets `banner`'s countdown or result for this `phase`. `won` is whether we
## (or our team) won; `winner_name` names a free-for-all winner.
static func fill(
	banner: Control, phase: String, info: Dictionary, won: bool, winner_name: String
) -> void:
	banner.title = ""
	banner.detail = ""
	banner.footnote = ""
	banner.plate = Color.TRANSPARENT
	banner.colour = UiTheme.TEXT
	banner.countdown = 0.0
	match phase:
		"countdown":
			banner.countdown = float(info.seconds_left)
			banner.countdown_mode = String(info.get("mode_name", "Team Deathmatch"))
			banner.countdown_objective = objective(info)
		"ended":
			var draw := int(info.winner) < 0
			banner.title = "DRAW" if draw else ("VICTORY" if won else "DEFEAT")
			banner.detail = reason(info, won, winner_name)
			banner.footnote = "Next match in %d" % int(ceil(info.seconds_left))
			if not draw:
				banner.plate = UiTheme.VICTORY if won else UiTheme.DEFEAT
				banner.colour = TeamColours.of(won)


## What to do this match, under the countdown.
static func objective(info: Dictionary) -> String:
	var limit := int(info.score_limit)
	if String(info.mode) == "gungame":
		var kills: int = info.get("kills_per_weapon", 1)
		return "Get %d kill%s with each of %d weapons" % [kills, "" if kills == 1 else "s", limit]
	return String(OBJECTIVES.get(String(info.mode), OBJECTIVES.tdm)) % limit


## Why the match ended, on the plate under the result.
static func reason(info: Dictionary, won: bool, winner_name: String) -> String:
	if info.get("free_for_all", false):
		if int(info.winner) < 0:
			return "Time limit reached"
		return "You finished first" if won else "%s finished first" % winner_name
	var scores: PackedInt32Array = info.scores
	if int(info.score_limit) > 0 and scores.size() == 2:
		if maxi(scores[0], scores[1]) >= int(info.score_limit):
			return "Score limit reached"
	return "Time limit reached"


## The team strip after a match: our side left, theirs right (none in a
## free-for-all).
static func strip(info: Dictionary) -> Dictionary:
	var scores: PackedInt32Array = info.scores
	var mine: int = info.my_team
	if info.get("free_for_all", false) or mine < 0 or scores.size() != 2:
		return {}
	return {
		"left":
		{"name": "Your team", "score": scores[mine], "team": mine, "colour": TeamColours.ally},
		"right":
		{
			"name": "Enemy",
			"score": scores[1 - mine],
			"team": 1 - mine,
			"colour": TeamColours.enemy,
		},
	}
