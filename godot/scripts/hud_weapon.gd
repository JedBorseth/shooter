extends Control
## The weapon cluster, bottom right: the weapon's name (and attachment) over
## its silhouette, a big magazine count beside a smaller reserve, and a strip
## with a tick for every round in the magazine (a plain bar for belt-fed
## weapons). It sits on a shade that fades in from the screen's corner.
##
## Low on ammo, the count and the strip turn red; while reloading they dim.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## Magazines bigger than this show a plain bar rather than a tick per round.
const MAX_TICKS := 40
const STRIP_WIDTH := 236.0

var player: FpsPlayer

## Brightens briefly when the weapon changes, so the swap reads.
var _flash := 0.0
var _key := ""


func _process(delta: float) -> void:
	if player == null:
		return
	var key := player.weapon_key()
	if key != _key:
		_key = key
		_flash = 1.0
	_flash = maxf(_flash - delta * 2.5, 0.0)
	queue_redraw()


func _draw() -> void:
	if player == null:
		return
	var right := size.x
	var bottom := size.y

	# Shade from the corner of the screen, under the whole cluster.
	Paint.corner_shade(
		self, Rect2(right - 520.0, bottom - 170.0, 560.0, 204.0), Vector2(1, 1), 0.55
	)

	var magazine := player.magazine()
	var reserve := player.reserve()
	var capacity := maxi(player.magazine_size(), 1)
	var low := magazine <= player.low_ammo_threshold()
	var reloading := player.is_reloading()
	var count_colour := UiTheme.ENEMY if low else UiTheme.TEXT
	if reloading:
		count_colour = Color(UiTheme.TEXT, 0.45)

	# Rounds strip along the bottom.
	var strip := Rect2(right - STRIP_WIDTH, bottom - 5.0, STRIP_WIDTH, 4.0)
	var strip_colour := UiTheme.ENEMY if low else Color(UiTheme.TEXT, 0.9)
	if reloading:
		strip_colour = Color(UiTheme.TEXT, 0.35)
	if capacity <= MAX_TICKS:
		var gap := 2.0 if capacity > 12 else 4.0
		Paint.segments(self, strip, capacity, float(magazine) / capacity, strip_colour, gap)
	else:
		Paint.segments(self, strip, 1, float(magazine) / capacity, strip_colour, 0.0)

	# Magazine, divider, reserve, right to left.
	var baseline := bottom - 16.0
	var reserve_w := Paint.text_right(
		self,
		Paint.LABEL,
		str(reserve),
		Vector2(right, baseline),
		24,
		Color(UiTheme.TEXT, 0.4 if reserve == 0 else 0.62)
	)
	var divider_x := right - reserve_w - 11.0
	draw_rect(Rect2(divider_x, baseline - 24.0, 1.5, 26.0), Color(UiTheme.TEXT, 0.35))
	var mag_w := Paint.text_right(
		self, Paint.DISPLAY, str(magazine), Vector2(divider_x - 10.0, baseline), 54, count_colour
	)
	var numbers_left := divider_x - 10.0 - mag_w

	# Silhouette left of the numbers; it brightens as the weapon comes up.
	var icon_colour := Color(1, 1, 1, lerpf(0.82, 1.0, _flash))
	Paint.weapon(self, _key, Vector2(numbers_left - 20.0, baseline - 18.0), 34.0, icon_colour)

	# Name, with the attachment after it in quieter type.
	var parts := Paint.split_weapon_name(_key, player.weapon_name())
	var name := parts[0].to_upper()
	var attachment := parts[1].to_upper()
	var name_line := bottom - 72.0
	var name_size := 18
	var tag_size := 13
	var name_w := Paint.text_width(Paint.LABEL, name, name_size, 1.5)
	var tag := ("  ·  " + attachment) if attachment != "" else ""
	var tag_w := Paint.text_width(Paint.LABEL_LIGHT, tag, tag_size, 1.0)
	var x := right - name_w - tag_w
	Paint.text(self, Paint.LABEL, name, Vector2(x, name_line), name_size, UiTheme.TEXT, 1.5)
	if tag != "":
		Paint.text(
			self,
			Paint.LABEL_LIGHT,
			tag,
			Vector2(x + name_w, name_line),
			tag_size,
			UiTheme.TEXT_DIM,
			1.0
		)
