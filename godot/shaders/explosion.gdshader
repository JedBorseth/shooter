// An explosion's fireball: camera-facing particles that play a flipbook of a
// fireball burning out into smoke (Unity Labs' Explosion02HD, CC0). While the
// frames are hot the fire glows (HDR emission, so the core blooms); as it
// cools the same frames turn into dark smoke lit by the scene, so the cloud
// sits in the map's light instead of glowing at night. It fades out where it
// meets the ground, so it has no hard edge there.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, diffuse_lambert_wrap;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform int frames_across = 5;
uniform int frames_down = 5;
/// How brightly the hot frames glow.
uniform float fire_energy = 3.2;
/// Over this distance (metres) in front of the ground or a wall the cloud
/// fades, so it has no hard edge where it meets them. Reading the depth
/// buffer costs a copy of it (about 0.1 ms on Medium), only in frames with
/// an explosion on screen.
uniform float soft_distance = 0.8;
uniform sampler2D depth_texture : hint_depth_texture, filter_nearest;
/// Animation progress (0 to 1) over which the fire cools to smoke.
uniform vec2 cooling = vec2(0.12, 0.5);

varying float progress;

void vertex() {
	// Face the camera, turned by the particle's angle, keeping its scale.
	mat4 world = mat4(
		normalize(INV_VIEW_MATRIX[0]),
		normalize(INV_VIEW_MATRIX[1]),
		normalize(INV_VIEW_MATRIX[2]),
		MODEL_MATRIX[3]
	);
	float a = INSTANCE_CUSTOM.x;
	world = world * mat4(
		vec4(cos(a), -sin(a), 0.0, 0.0),
		vec4(sin(a), cos(a), 0.0, 0.0),
		vec4(0.0, 0.0, 1.0, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
	MODELVIEW_MATRIX = VIEW_MATRIX * world * mat4(
		vec4(length(MODEL_MATRIX[0].xyz), 0.0, 0.0, 0.0),
		vec4(0.0, length(MODEL_MATRIX[1].xyz), 0.0, 0.0),
		vec4(0.0, 0.0, length(MODEL_MATRIX[2].xyz), 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
	MODELVIEW_NORMAL_MATRIX = mat3(MODELVIEW_MATRIX);
	progress = clamp(INSTANCE_CUSTOM.z, 0.0, 1.0);
	float total = float(frames_across * frames_down);
	float frame = min(floor(progress * total), total - 1.0);
	UV = (UV + vec2(mod(frame, float(frames_across)), floor(frame / float(frames_across))))
		/ vec2(float(frames_across), float(frames_down));
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	float heat = 1.0 - smoothstep(cooling.x, cooling.y, progress);
	// The warm, bright parts of a frame are flame; the rest is smoke.
	float flame = smoothstep(0.08, 0.45, tex.r - tex.b) * heat;
	ALBEDO = mix(tex.rgb * 1.1 + vec3(0.09, 0.08, 0.07), vec3(0.0), flame);
	EMISSION = tex.rgb * flame * fire_energy;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float depth = textureLod(depth_texture, SCREEN_UV, 0.0).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	behind.xyz /= behind.w;
	float soft = clamp((VERTEX.z - behind.z) / soft_distance, 0.0, 1.0);
	ALPHA = tex.a * COLOR.a * soft;
}
