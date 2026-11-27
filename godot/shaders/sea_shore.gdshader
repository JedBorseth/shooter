shader_type spatial;
// Where Breakwater's harbour water meets the quay face, the rock revetment,
// and the moored tanker's hull (tools/breakwater_layout.py `shore`): a strip
// laid a few millimetres over the sea plane, its UV.y running from 0 at the
// wall to 1 at its outer edge. It blends a broken line of foam and churned,
// murkier water over the sea along the wall, fading out across the strip, so
// the water reads as touching the wall rather than passing under it. Cheap:
// two texture reads on a narrow strip, no depth or screen reads.
render_mode blend_mix, depth_draw_never, cull_back, specular_schlick_ggx;

// The churned water by the wall: brown-green silt.
uniform vec3 murk_color : source_color = vec3(0.07, 0.09, 0.07);
uniform vec3 foam_color : source_color = vec3(0.78, 0.8, 0.78);
uniform sampler2D chop : hint_normal, filter_linear_mipmap, repeat_enable;
uniform float chop_size = 7.0;
// Metres of strip the foam reaches out from the wall.
uniform float foam_reach : hint_range(0.0, 1.0) = 0.22;
// Breakup of the foam line: tiling value noise (the world's macro noise).
global uniform sampler2D world_noise : filter_linear_mipmap, repeat_enable;

varying vec3 world_pos;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec2 p = world_pos.xz;
	float out_ = clamp(UV.y, 0.0, 1.0);
	// Two scrolling reads of the noise: the foam gathers and breaks in clumps
	// that drift along the wall.
	float n1 = texture(world_noise, p * 0.09 + vec2(TIME * 0.004, -TIME * 0.003)).r;
	float n2 = texture(world_noise, p * 0.31 + vec2(-TIME * 0.009, TIME * 0.006)).r;
	float clumps = smoothstep(0.38, 0.72, n1 * 0.6 + n2 * 0.55);
	// A thin hard line right at the wall (the lapping edge), then clumps out
	// to foam_reach, then nothing.
	float edge = 1.0 - smoothstep(0.0, 0.05, out_);
	float foam = max(edge * (0.55 + 0.45 * n2), clumps * (1.0 - smoothstep(0.0, foam_reach, out_)) * 0.85);
	float murk = (1.0 - smoothstep(0.0, 1.0, out_)) * 0.55;
	vec3 b = texture(chop, p / chop_size + vec2(-TIME * 0.011, TIME * 0.009)).xyz * 2.0 - 1.0;
	NORMAL_MAP = normalize(vec3(b.xy * 0.6, 1.0)) * 0.5 + 0.5;
	NORMAL_MAP_DEPTH = 0.4;
	ALBEDO = mix(murk_color, foam_color, foam);
	ALPHA = clamp(max(murk, foam), 0.0, 1.0);
	ROUGHNESS = mix(0.12, 0.85, foam);
	SPECULAR = 0.5;
}
