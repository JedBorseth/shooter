// Particle flames: each a camera-facing tongue, wide at its root and licking
// to a point, its edge and body broken by noise that scrolls upward, so a few
// dozen of them read as fire rather than glowing discs. Additive; the colour
// (HDR) and fade come from the particle (COLOR); the quad keeps the
// particle's scale.
shader_type spatial;
render_mode blend_add, depth_draw_never, cull_disabled, unshaded, shadows_disabled;

uniform sampler2D noise_tex : hint_default_white, filter_linear_mipmap, repeat_enable;

varying vec2 seed;

void vertex() {
	// Face the camera, keeping the particle's own scale.
	mat4 billboard = mat4(INV_VIEW_MATRIX[0], INV_VIEW_MATRIX[1], INV_VIEW_MATRIX[2], MODEL_MATRIX[3]);
	float scale = length(MODEL_MATRIX[0].xyz);
	MODELVIEW_MATRIX = VIEW_MATRIX * billboard * mat4(
		vec4(scale, 0.0, 0.0, 0.0), vec4(0.0, scale, 0.0, 0.0), vec4(0.0, 0.0, scale, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0));
	float id = float(INSTANCE_ID);
	seed = vec2(fract(id * 0.618), fract(id * 0.381));
}

void fragment() {
	// h: 0 at the root, 1 at the tip.
	float h = 1.0 - UV.y;
	vec2 nuv = vec2(UV.x * 0.9 + seed.x, UV.y * 0.7 + seed.y + TIME * 1.1);
	float n = texture(noise_tex, nuv).r;
	float n2 = texture(noise_tex, nuv * 2.3 + vec2(0.31, TIME * 0.7)).r;
	// The tongue narrows toward the tip and sways with the noise as it rises.
	float width = mix(0.36, 0.03, pow(h, 0.75));
	float sway = (n - 0.5) * 0.35 * h;
	float edge = abs(UV.x - 0.5 + sway) / width;
	float body = 1.0 - smoothstep(0.45, 1.0, edge + (n2 - 0.5) * 0.5);
	body *= smoothstep(0.0, 0.15, h) * (1.0 - smoothstep(0.7, 1.0, h));
	// Torn into licks: holes open toward the tip.
	body *= smoothstep(0.15 + 0.45 * h, 0.55 + 0.35 * h, n * 0.6 + n2 * 0.4 + 0.25);
	// A hotter core.
	float core = 1.0 - smoothstep(0.0, 0.55, edge);
	ALBEDO = COLOR.rgb * mix(0.55, 1.25, core);
	ALPHA = clamp(COLOR.a * body, 0.0, 1.0);
}
