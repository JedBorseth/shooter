// A soft puff of gun smoke, drawn on particles from the muzzle: a frame of
// the smoke flipbook (Unity Labs' WispySmoke01, CC0), facing the camera and
// lit by the scene, so it is pale in the sun, dark in shade, and never glows
// at night. Drawn with the world camera's projection at the world position of
// the muzzle as the player sees it (the first-person weapon's emitters are
// placed there each shot), so the puff stays where it was fired as the view
// moves and walls hide it. Fades with the particle's alpha.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, diffuse_lambert_wrap;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform vec4 tint : source_color = vec4(0.62, 0.62, 0.64, 1.0);
uniform float opacity = 0.55;

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
}
