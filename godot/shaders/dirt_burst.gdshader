// A jet of dirt and dust an explosion throws up out of the ground: a quad
// standing on its foot (the particle's origin) along the particle's Y axis
// (its flight, with align_y), turned about that axis to face the camera, that
// plays the Blender-rendered burst (`assets/textures/effects/dirt_burst_4x4.png`,
// `tools/build_explosion_fx.py`). The frames go from a dense, dark column to a
// torn mushroom of clumps to thin dust; the flipbook runs fast at first and
// slows, as the dirt does. The colour is the frame's own shading (from a sun
// above) times the dirt's tint, lit by the scene per vertex, so it is brown in
// the sun, dark in shade, and never glows at night. It fades in over its
// lowest `ground_fade` metres, so its foot has no hard line on the ground.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, vertex_lighting, skip_vertex_transform;

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform vec4 tint : source_color = vec4(0.36, 0.31, 0.26, 1.0);
/// How dark the shadowed side of a billow gets, against its lit side.
uniform float shadow = 0.4;
uniform float opacity = 1.0;
/// The flipbook's progress is the life gone raised to this (under 1 runs
/// fast at first, then slows).
uniform float ease = 0.6;
uniform float ground_fade = 0.5;
/// The quad mesh's lowest local height (its `center_offset.y` less half its
/// height): the jet's foot.
uniform float foot = -0.05;
/// The ground the blast went off on (world height), per explosion.
instance uniform float ground_y = -1000.0;

varying float world_y;
varying float fade;

void vertex() {
	vec3 axis = MODEL_MATRIX[1].xyz;
	float height = max(length(axis), 0.0001);
	axis /= height;
	float width = length(MODEL_MATRIX[0].xyz);
	vec3 base = MODEL_MATRIX[3].xyz;
	vec3 to_camera = normalize(CAMERA_POSITION_WORLD - base);
	vec3 side = cross(axis, to_camera);
	side = length(side) > 0.001 ? normalize(side) : vec3(1.0, 0.0, 0.0);
	// Mirror every other jet, so the fan does not repeat one shape.
	float flip = mod(float(INSTANCE_ID), 2.0) < 0.5 ? 1.0 : -1.0;
	float life = clamp(INSTANCE_CUSTOM.y, 0.0, 1.0);
	float frame = min(floor(pow(life, ease) * 16.0), 15.0);
	// Early frames fill only the foot of their cell: shrink the quad to the
	// part of the frame the jet reaches (measured from the atlas; the frames
	// grow as 1 - (1 - frame / 15)^2.4), so the empty rest costs no fill.
	float grown = 1.0 - pow(1.0 - frame / 15.0, 2.4);
	float tall = min(0.36 + 0.62 * grown, 1.0);
	float wide = min(0.6 + 0.6 * grown, 1.0);
	vec2 local = vec2(VERTEX.x * wide, foot + (VERTEX.y - foot) * tall);
	vec2 uv = vec2(0.5 + (UV.x - 0.5) * wide, 1.0 - (1.0 - UV.y) * tall);
	vec3 world = base + axis * local.y * height + side * local.x * width * flip;
	world_y = world.y;
	// Built in view space here, so the scene's light reaches the quad as it
	// stands, facing the camera.
	VERTEX = (VIEW_MATRIX * vec4(world, 1.0)).xyz;
	NORMAL = normalize((VIEW_MATRIX * vec4(cross(side, axis), 0.0)).xyz);
	UV = (uv + vec2(mod(frame, 4.0), floor(frame / 4.0))) * 0.25;
	// Fade the last fifth of the life away.
	fade = clamp((1.0 - life) / 0.2, 0.0, 1.0);
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	ALBEDO = tint.rgb * mix(shadow, 1.0, tex.r);
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float ground = clamp((world_y - ground_y) / ground_fade, 0.0, 1.0);
	ALPHA = tex.a * opacity * COLOR.a * fade * ground;
}
