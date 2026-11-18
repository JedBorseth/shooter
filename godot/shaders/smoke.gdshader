// Particle smoke: each puff a camera-facing quad whose edge is carved by
// noise (offset per particle, and drifting as it ages), so a column of puffs
// reads as billowing smoke rather than soft discs. Lit, so the low sun warms
// one side. Colour and opacity come from the particle (COLOR); the quad keeps
// the particle's scale.
shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_disabled, diffuse_lambert, specular_disabled;

uniform sampler2D noise_tex : hint_default_white, filter_linear_mipmap, repeat_enable;
// How much of the quad's radius the puff fills before the noise eats it.
uniform float softness : hint_range(0.0, 1.0) = 0.55;

varying vec2 offset;

void vertex() {
	// Face the camera, keeping the particle's own scale.
	mat4 billboard = mat4(INV_VIEW_MATRIX[0], INV_VIEW_MATRIX[1], INV_VIEW_MATRIX[2], MODEL_MATRIX[3]);
	float scale = length(MODEL_MATRIX[0].xyz);
	MODELVIEW_MATRIX = VIEW_MATRIX * billboard * mat4(
		vec4(scale, 0.0, 0.0, 0.0), vec4(0.0, scale, 0.0, 0.0), vec4(0.0, 0.0, scale, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0));
	MODELVIEW_NORMAL_MATRIX = mat3(MODELVIEW_MATRIX);
	// A different patch of noise per puff, sliding as it ages (CUSTOM.y).
	float id = float(INSTANCE_ID);
	offset = vec2(fract(id * 0.618), fract(id * 0.381)) + vec2(INSTANCE_CUSTOM.y * 0.15, 0.0);
}

void fragment() {
	float r = length(UV - 0.5) * 2.0;
	float body = 1.0 - smoothstep(softness * 0.4, 1.0, r);
	float n = texture(noise_tex, UV * 0.7 + offset).r;
	float carved = smoothstep(0.25, 0.7, n * 0.75 + body * 0.6) * body;
	ALBEDO = COLOR.rgb;
	ALPHA = COLOR.a * carved;
	ROUGHNESS = 1.0;
}
