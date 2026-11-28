// An explosion's core: camera-facing particles that play a flipbook of a
// fireball burning out into smoke (Unity Labs' Explosion02HD, CC0), from
// `first_frame` on (the source's first frames are a white-hot ball, which
// under the bloom read as a white puff rather than a blast). While the frames
// are hot the fire glows orange to yellow, never white, at a bounded HDR
// brightness so the bloom gives it a short halo without blowing out the
// picture; as it cools the same frames turn into dark smoke lit by the scene,
// so the cloud sits in the map's light instead of glowing at night. It fades
// out where it meets the ground, so it has no hard edge there.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, diffuse_lambert_wrap;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform int frames_across = 5;
uniform int frames_down = 5;
/// The first frame played, and the last (-1: the flipbook's last).
uniform int first_frame = 0;
uniform int last_frame = -1;
/// How brightly the hot frames glow.
uniform float fire_energy = 3.2;
/// The fire's colour from its dim edges to its hottest core.
uniform vec3 fire_edge : source_color = vec3(0.9, 0.32, 0.06);
uniform vec3 fire_core : source_color = vec3(1.0, 0.72, 0.36);
/// Over this distance (metres) in front of the ground or a wall the cloud
/// fades, so it has no hard edge where it meets them. Reading the depth
/// buffer costs a copy of it (about 0.1 ms on Medium), only in frames with
/// an explosion on screen.
uniform float soft_distance = 0.8;
uniform sampler2D depth_texture : hint_depth_texture, filter_nearest;
/// Animation progress (0 to 1) over which the fire cools to smoke.
uniform vec2 cooling = vec2(0.12, 0.5);
/// Animation progress over which the particle fades out (a frag's core
/// gives way to the dirt it throws up instead of hanging as a ball).
uniform vec2 fade_out = vec2(1.0, 1.0);
/// How much of a frame's bright, colourless middle counts as flame. The
/// source's first frames are a white-hot ball with little colour in it: the
/// blast's opening flash plays them with this at 1, so they burn white.
uniform float white_heat = 0.0;

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
	float first = float(first_frame);
	float last = last_frame < 0 ? total - 1.0 : float(last_frame);
	float frame = min(first + floor(progress * (last + 1.0 - first)), last);
	UV = (UV + vec2(mod(frame, float(frames_across)), floor(frame / float(frames_across))))
		/ vec2(float(frames_across), float(frames_down));
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	float heat = 1.0 - smoothstep(cooling.x, cooling.y, progress);
	// The warm, bright parts of a frame are flame; the rest is smoke.
	float hot = clamp(dot(tex.rgb, vec3(0.3, 0.5, 0.2)), 0.0, 1.0);
	float flame = max(smoothstep(0.08, 0.45, tex.r - tex.b), smoothstep(0.25, 0.7, hot) * white_heat)
		* heat;
	ALBEDO = mix(tex.rgb * 1.1 + vec3(0.09, 0.08, 0.07), vec3(0.0), flame);
	EMISSION = mix(fire_edge, fire_core, hot * hot) * hot * flame * fire_energy;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float depth = textureLod(depth_texture, SCREEN_UV, 0.0).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	behind.xyz /= behind.w;
	float soft = clamp((VERTEX.z - behind.z) / soft_distance, 0.0, 1.0);
	float gone = fade_out.x < 1.0 ? smoothstep(fade_out.x, fade_out.y, progress) : 0.0;
	ALPHA = tex.a * COLOR.a * soft * (1.0 - gone);
}
