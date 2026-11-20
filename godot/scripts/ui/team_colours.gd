extends RefCounted
## The colours that tell your side from the enemy's everywhere they appear:
## the score block, kill feed, scoreboard, minimap, objective markers, and
## mode HUDs. Settings, Interface, "Team colours" picks a palette for colour
## vision deficiency, as Call of Duty's colour customisation does
## (protanopia, deuteranopia, tritanopia). `Settings` sets the palette; HUD
## pieces read `ally` and `enemy` each time they draw.
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## Palette names, by the setting's index.
const NAMES: Array[String] = ["Default", "Protanopia", "Deuteranopia", "Tritanopia"]
## [ally, enemy] for each palette. Default is the theme's blue against red.
## Protanopia and deuteranopia keep blue and move the enemy to an orange that
## stays apart from it (from Okabe and Ito's palette for colour-blind
## readers); protanopia's orange is lighter, because red-blind eyes see red
## light as dim. Tritanopia, which confuses blue with green, moves allies to
## cyan-free teal and keeps the enemy a pinkish red.
const PALETTES: Array[Array] = [
	[UiTheme.ALLY, UiTheme.ENEMY],
	[Color("56b4e9"), Color("ffb347")],
	[Color("56b4e9"), Color("e69f00")],
	[Color("2fd6b4"), Color("ff4f7b")],
]

static var ally: Color = UiTheme.ALLY
static var enemy: Color = UiTheme.ENEMY


## Switches to palette `index` (into NAMES).
static func use(index: int) -> void:
	var palette: Array = PALETTES[clampi(index, 0, PALETTES.size() - 1)]
	ally = palette[0]
	enemy = palette[1]


## The colour for a side: ours or theirs.
static func of(ours: bool) -> Color:
	return ally if ours else enemy
