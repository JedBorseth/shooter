// Fire: flames that play a looping flipbook (Unity Labs' SmallFlame01, CC0),
// turned about the vertical to face the camera so they stand upright, glowing
// in HDR so their cores bloom. Each particle starts the loop at its own frame.
// They fade out where they meet the ground, so no hard edge shows there.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform int frames_across = 8;
uniform int frames_down = 4;
/// Frames a second the loop plays at.
uniform float speed = 24.0;
uniform float energy = 2.4;
uniform float soft_distance = 0.25;
uniform sampler2D depth_texture : hint_depth_texture, filter_nearest;

void vertex() {
	// Upright: face the camera by turning about world up only.
	vec3 centre = MODEL_MATRIX[3].xyz;
	vec3 to_camera = CAMERA_POSITION_WORLD - centre;
	to_camera.y = 0.0;
	vec3 forward = length(to_camera) > 0.001 ? normalize(to_camera) : vec3(0.0, 0.0, 1.0);
	vec3 right = normalize(cross(vec3(0.0, 1.0, 0.0), forward));
	float width = length(MODEL_MATRIX[0].xyz);
	float height = length(MODEL_MATRIX[1].xyz);
	// The quad's foot sits on the particle, so flames rise from where they burn.
	vec3 world = centre + right * VERTEX.x * width + vec3(0.0, 1.0, 0.0) * (VERTEX.y + 0.5) * height;
	VERTEX = (VIEW_MATRIX * vec4(world, 1.0)).xyz;
	float total = float(frames_across * frames_down);
	float frame = mod(floor(TIME * speed + float(INSTANCE_ID) * 7.0), total);
	UV = (UV + vec2(mod(frame, float(frames_across)), floor(frame / float(frames_across))))
		/ vec2(float(frames_across), float(frames_down));
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	float depth = textureLod(depth_texture, SCREEN_UV, 0.0).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	behind.xyz /= behind.w;
	float soft = clamp((VERTEX.z - behind.z) / soft_distance, 0.0, 1.0);
	ALBEDO = tex.rgb * tex.a * energy * COLOR.rgb * COLOR.a * soft;
}
