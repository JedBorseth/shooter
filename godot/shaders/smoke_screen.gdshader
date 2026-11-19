// A billow of a smoke grenade's cloud (or a flashbang's puff): a camera-facing
// particle that plays one of two billowing loops from Unity Labs' CC0 Cloud
// flipbooks (`assets/textures/effects/cloud_8x4.png`, two rows of eight
// frames per loop), cross-fading between frames so it churns slowly instead
// of stepping. Lit by the scene per vertex (the billows are large and soft,
// so per-pixel light would only cost more), so the bank is pale in the sun,
// grey in shade, and never glows at night. It thins out over its lowest
// `ground_fade` metres above the ground it popped on, so the bank has no hard
// line where the billows cross the floor. (A depth-faded edge against walls,
// as High gives gun smoke, is left out: it needs a copy of the depth buffer
// that frames with nobody firing do not otherwise make.)
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, vertex_lighting;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform vec4 tint : source_color = vec4(0.8, 0.8, 0.79, 1.0);
uniform float opacity = 0.9;
/// Frames a second each billow churns through (16 frames a loop).
uniform float frame_rate = 2.0;
/// Billow size, times the particle's own scale (set per preset).
uniform float size_scale = 1.0;
/// Over this height (metres) above `ground_y` the billows fade in.
uniform float ground_fade = 0.9;
/// The ground the cloud popped on (world height), per cloud.
instance uniform float ground_y = -1000.0;

varying float blend;
varying vec2 next_uv;
varying float world_y;

void vertex() {
	float size = length(MODEL_MATRIX[0].xyz) * size_scale;
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
	world_y = (world * vec4(VERTEX, 1.0)).y;
	// Which loop, where in it to start, and how far it has churned
	// (INSTANCE_CUSTOM.y is the fraction of its life gone, .w its lifetime).
	float loop = step(0.5, fract(seed * 7.31));
	float time = seed * 16.0 + INSTANCE_CUSTOM.y * INSTANCE_CUSTOM.w * frame_rate;
	float frame = mod(floor(time), 16.0);
	float after = mod(frame + 1.0, 16.0);
	// Hold each frame, then cross-fade to the next over the last 30% of its
	// time: most billows fetch one frame (the branch is the same across a
	// whole quad, so it costs nothing to take).
	blend = smoothstep(0.7, 1.0, fract(time));
	vec2 cell = vec2(1.0 / 8.0, 1.0 / 4.0);
	vec2 base = UV;
	UV = (base + vec2(mod(frame, 8.0), loop * 2.0 + floor(frame / 8.0))) * cell;
	next_uv = (base + vec2(mod(after, 8.0), loop * 2.0 + floor(after / 8.0))) * cell;
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	if (blend > 0.0) {
		tex = mix(tex, texture(flipbook, next_uv), blend);
	}
	ALBEDO = tint.rgb * tex.rgb;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float ground = clamp((world_y - ground_y) / ground_fade, 0.0, 1.0);
	ALPHA = tex.a * opacity * COLOR.a * ground;
}
