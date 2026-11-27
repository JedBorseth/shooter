extends RefCounted
## The words on the Options screen: what each tab holds, and what each setting
## does, for the description pane beside the list (`pause_menu.gd`).
##
## Load it with `preload`; everything here is constant.

## What each tab holds, shown before a setting is picked.
const TAB_INTROS: Array[String] = [
	(
		"What you play with, and how it aims. The device you don't pick is locked out"
		+ " of play. Pick a setting to see what it does."
	),
	"Choose a binding, then press a key or mouse button. A key used by another action swaps with it.",
	"Display and quality. Every change applies right away.",
	"Volume for each kind of sound, and how it is mixed.",
	"Your name, and what the screen shows while you play.",
]
## What each setting does, for the pane beside the list.
const DESCRIPTIONS := {
	&"input_device":
	(
		"Play with keyboard and mouse or with a controller. The other one is locked out"
		+ " while you play, so nothing switches by accident and aim assist stays with"
		+ " controllers. Menus and pause work with both."
	),
	&"mouse_sensitivity": "How far the view turns for each movement of the mouse.",
	&"ads_sensitivity":
	"Turn speed while aiming down sights, as a share of your normal sensitivity.",
	&"invert_mouse_y": "Moving the mouse forward looks down instead of up.",
	&"stick_sensitivity_level": "How fast the right stick turns the view. Each step is 20% faster.",
	&"stick_deadzone":
	"How far the right stick must move before the view turns. Raise it if the view drifts.",
	&"move_deadzone":
	"How far the left stick must move before you walk. Raise it if you creep on your own.",
	&"trigger_deadzone": "How far a trigger must be pulled to fire or aim. Lower is quicker.",
	&"stick_response":
	(
		"How turn speed builds as you push the right stick. Standard is finer near the"
		+ " centre, Linear turns in step with the stick, and Dynamic is an S-curve: fine"
		+ " at first, quick through the middle."
	),
	&"invert_stick_y": "Pushing the right stick up looks down.",
	&"stick_curve":
	"How strongly the Standard curve holds back small movements. Higher is finer near the centre.",
	&"aim_assist":
	(
		"With a controller, slows your aim over an enemy and follows them as you move."
		+ " Mouse aiming never gets aim assist."
	),
	&"rumble": "Controller vibration for shots, hits, and explosions.",
	&"button_prompts":
	(
		"Which controller's buttons prompts show. Auto matches the connected controller;"
		+ " keyboard keys and mouse buttons always show as they are."
	),
	&"fov":
	(
		"How much of the world you see across the screen. Wider shows more at the edges"
		+ " and makes targets smaller. Your weapon keeps its own field of view, so it never"
		+ " stretches."
	),
	&"fullscreen": "Fill the screen instead of running in a window.",
	&"vsync": "Match frames to the display to stop tearing. Adds a little input delay.",
	&"max_fps": "Cap the frame rate to keep the machine cooler and quieter.",
	&"graphics_quality":
	(
		"Shadows, lighting, and effects in one choice. Lower presets run faster on older"
		+ " graphics cards."
	),
	&"render_scale":
	"Draws the 3D world at a lower resolution and scales it up. The HUD stays sharp.",
	&"motion_blur": "Blurs fast camera turns.",
	&"lens_effects": "Darkens the corners of the picture and adds a little film grain.",
	&"master_volume": "Every sound in the game.",
	&"weapons_volume": "Gunfire, reloads, and weapon handling.",
	&"effects_volume": "Impacts, explosions, and other effects.",
	&"footsteps_volume": "Footsteps, yours and everyone else's.",
	&"ambience_volume": "Wind, city noise, and other background sound.",
	&"ui_volume": "Menu clicks and HUD sounds.",
	&"audio_mix":
	(
		"Headphones keeps full stereo. Speakers narrows it. Competitive also evens out loud"
		+ " and quiet sounds so footsteps carry. Night turns gunfire and explosions down"
		+ " and evens everything out, for playing quietly."
	),
	&"subtitles": "Captions for calls and sound cues, such as the 30-second warning.",
	&"player_name": "The name other players see in the kill feed and on the scoreboard.",
	&"camera_motion": "Head bob and weapon sway while you move.",
	&"show_fps": "Shows the frame rate in the corner of the screen.",
	&"skip_killcams":
	(
		"Respawn without watching the replay of each death. The kill that ends a round or"
		+ " the match still plays for everyone."
	),
	&"team_colours":
	(
		"Your side and the enemy's on the HUD, minimap, kill feed, and objectives. The"
		+ " palettes for protanopia, deuteranopia, and tritanopia keep the two apart."
	),
	&"hud_scale": "Size of the HUD's corner pieces: health, ammo, score, minimap, and feeds.",
	&"hud_opacity": "How solid the HUD is. The crosshair and hit markers stay as they are.",
	&"minimap_rotates":
	"The minimap turns as you do, so you always face up. Off keeps north at the top.",
	&"bindings":
	(
		"Choose a binding, then press a key or mouse button. A key used by another action"
		+ " swaps with it."
	),
}
