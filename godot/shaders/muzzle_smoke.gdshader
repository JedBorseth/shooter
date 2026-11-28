// A soft puff of gun smoke, drawn on particles from the muzzle: a frame of
// the smoke flipbook (Unity Labs' WispySmoke01, CC0), facing the camera and
// lit by the scene, so it is pale in the sun, dark in shade, and never glows
// at night. Drawn with the world camera's projection at the world position of
// the muzzle as the player sees it (the first-person weapon's emitters are
// placed there each shot), so the puff stays where it was fired as the view
// moves and walls hide it. Fades with the particle's alpha. On High
// (SOFT_SMOKE, rewritten by GraphicsQuality per preset) a puff also fades out
// where it meets the world, instead of cutting a hard line across a wall or
// the ground: it reads the depth buffer, which the firefight measurement
// (tests/combat_fx.gd) found already copied while anything fires.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, diffuse_lambert_wrap;

// 1 compiles in the soft edge against the world (High). The graphics preset
// rewrites this line (GraphicsQuality.set_soft_smoke); keep its form.
#define SOFT_SMOKE 0

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform vec4 tint : source_color = vec4(0.7, 0.7, 0.72, 1.0);
uniform float opacity = 0.85;
#if SOFT_SMOKE
uniform sampler2D depth_tex : hint_depth_texture, filter_nearest;
// The puff fades out over this many metres in front of what is behind it.
const float SOFT_DEPTH = 0.35;
#endif

void vertex() {
	// Face the camera, turned by a per-particle angle, keeping its scale.
	float size = length(MODEL_MATRIX[0].xyz);
	// The process material gives each puff a random angle (radians).
	float a = INSTANCE_CUSTOM.x;
	float seed = fract(a * 0.1591549 + 0.5);
	mat4 world = mat4(
		normalize(INV_VIEW_MATRIX[0]) * size,
		normalize(INV_VIEW_MATRIX[1]) * size,
		normalize(INV_VIEW_MATRIX[2]) * size,
		MODEL_MATRIX[3]
	);
	world = world * mat4(
		vec4(cos(a), -sin(a), 0.0, 0.0),
		vec4(sin(a), cos(a), 0.0, 0.0),
		vec4(0.0, 0.0, 1.0, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
	MODELVIEW_MATRIX = VIEW_MATRIX * world;
	MODELVIEW_NORMAL_MATRIX = mat3(MODELVIEW_MATRIX);
	// One of the sixteen frames, per particle.
	float frame = floor(seed * 16.0);
	UV = (UV + vec2(mod(frame, 4.0), floor(frame / 4.0))) * 0.25;
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	ALBEDO = tint.rgb * tex.rgb;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	ALPHA = tex.a * opacity * COLOR.a;
#if SOFT_SMOKE
	float raw = texture(depth_tex, SCREEN_UV).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw, 1.0);
	// View-space depths are negative: the gap between the world and the puff.
	ALPHA *= clamp((VERTEX.z - behind.z / behind.w) / SOFT_DEPTH, 0.0, 1.0);
#endif
}
