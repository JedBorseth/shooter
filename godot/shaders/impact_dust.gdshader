// A bullet impact's dust puff: a camera-facing particle playing the smoke
// flipbook (`assets/textures/effects/smoke_4x4.png`, Unity Labs' CC0
// WispySmoke01) in the colour of the surface that was hit (the particle's
// colour). Lit by the scene per vertex, with normals tipped from the camera
// toward the sky: a flat billboard's normal faces the camera, so a puff seen
// from below (a wall above the player) or against the light went charcoal
// grey, where real dust scatters the sky and sun and stays pale.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, vertex_lighting, skip_vertex_transform, diffuse_lambert_wrap;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform int frames_across = 4;
uniform int frames_down = 4;
/// How far the normals tip from the camera toward the sky (0 to 1).
uniform float sky_normal = 0.7;

void vertex() {
	float size = length(MODEL_MATRIX[0].xyz);
	float a = INSTANCE_CUSTOM.x;
	vec2 corner = vec2(
		cos(a) * VERTEX.x - sin(a) * VERTEX.y,
		sin(a) * VERTEX.x + cos(a) * VERTEX.y
	) * size;
	vec3 right = normalize(INV_VIEW_MATRIX[0].xyz);
	vec3 up = normalize(INV_VIEW_MATRIX[1].xyz);
	vec3 world = MODEL_MATRIX[3].xyz + right * corner.x + up * corner.y;
	VERTEX = (VIEW_MATRIX * vec4(world, 1.0)).xyz;
	vec3 facing = normalize(INV_VIEW_MATRIX[2].xyz);
	NORMAL = normalize(mat3(VIEW_MATRIX) * normalize(mix(facing, vec3(0.0, 1.0, 0.0), sky_normal)));
	// The particle's animation progress picks the frame, played once.
	float total = float(frames_across * frames_down);
	float frame = clamp(floor(INSTANCE_CUSTOM.z * total), 0.0, total - 1.0);
	UV = (UV + vec2(mod(frame, float(frames_across)), floor(frame / float(frames_across))))
		/ vec2(float(frames_across), float(frames_down));
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	ALBEDO = COLOR.rgb * tex.rgb;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	ALPHA = COLOR.a * tex.a;
}
