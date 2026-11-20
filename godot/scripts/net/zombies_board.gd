extends RefCounted
## Zombies' scoreboard for the match HUD's board (`tdm_hud.gd`): one team,
## ordered by points, with kills, downs, revives, and headshots, as Black
## Ops' co-op scoreboard. The dead (bled out, or waiting for the next round)
## show faint.

const HEADINGS := ["NAME", "POINTS", "KILLS", "DOWNS", "REVIVES", "HEADSHOTS", "PING"]


## The board's sections for `roster` (`NetClient.roster()`), in `colour`.
static func sections(roster: Array, colour: Color) -> Array:
	var entries := roster.duplicate()
	entries.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.score > b.score)
	var rows := []
	for entry: Dictionary in entries:
		var cells := [
			entry.name,
			str(entry.score),
			str(entry.kills),
			str(entry.deaths),
			str(entry.get("revives", 0)),
			str(entry.get("headshots", 0)),
			"—" if entry.bot else str(entry.ping),
		]
		(
			rows
			. append(
				{
					"cells": cells,
					"me": entry.me,
					"faint": not entry.alive,
					"tag": "BOT" if entry.bot else "",
				}
			)
		)
	return [
		{
			"name": "SURVIVORS",
			"score": "",
			"colour": colour,
			"headings": HEADINGS,
			"rows": rows,
		}
	]
