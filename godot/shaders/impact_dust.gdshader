// A bullet impact's dust puff (or a hit's blood mist): a camera-facing
// particle playing a puff rendered in Blender
// (`assets/textures/effects/impact_puff_{a,b}.png`, `tools/build_flipbooks.py`:
// 256-pixel cells, three variants of 16 frames, two atlas rows each: fine
// dust off concrete, plaster and wood, a clumpier dirt burst, and a blood
// mist; `variant` picks one per impact), once over the particle's life,
// stretched along its flight so it bursts out of the surface, in
// the colour of the surface that was hit (the particle's colour). The
// flipbook is a six-way lightmap (`sixway.gdshaderinc`): the puff takes the
// sun on the side it shines from (`sun_direction`, set by `impact.gd`) and
// shades itself on the other. Lit by the scene per vertex, with normals
// tipped from the camera toward the sky: a flat billboard's normal faces the
// camera, so a puff seen from below or against the light went charcoal,
// where real dust scatters the sky and sun and stays pale.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, vertex_lighting, skip_vertex_transform, diffuse_lambert_wrap;

#include "res://shaders/sixway.gdshaderinc"

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform sampler2D flipbook_b : source_color, filter_linear_mipmap, repeat_disable;
/// How far the normals tip from the camera toward the sky (0 to 1).
uniform float sky_normal = 0.7;
/// The direction toward the sun (world).
uniform vec3 sun_direction = vec3(0.35, 0.8, 0.45);
/// How much of the puff's shading follows the sun (the rest is the sky).
uniform float sun_share = 0.6;
/// The flipbook's light scaled to the surface colour's brightness.
uniform float light_gain = 2.2;
/// How much longer than wide a puff seen side-on is drawn, along its flight.
uniform float stretch_along = 1.6;
/// Which puff: 0 dust, 1 dirt, 2 blood mist.
instance uniform float variant = 0.0;

varying vec3 sun_cell;

void vertex() {
	// The particle's Y follows its flight (align_y): the puff is stretched
	// along that flight as it shows on screen, its foot at the particle, so
	// it bursts out of the surface instead of sitting on it as a disc. Seen
	// along its flight (toward or away from the camera) it turns back into a
	// round puff.
	float size = length(MODEL_MATRIX[0].xyz);
	vec3 origin = MODEL_MATRIX[3].xyz;
	vec3 to_camera = normalize(CAMERA_POSITION_WORLD - origin);
	vec3 axis = MODEL_MATRIX[1].xyz / max(length(MODEL_MATRIX[1].xyz), 1e-4);
	vec3 along = axis - to_camera * dot(axis, to_camera);
	float side_on = smoothstep(0.15, 0.6, length(along));
	vec3 up = normalize(mix(INV_VIEW_MATRIX[1].xyz, along / max(length(along), 1e-4), side_on));
	vec3 right = normalize(cross(up, to_camera));
	// Every other puff mirrored, so a burst of them does not repeat.
	float mirror = INSTANCE_CUSTOM.x > 0.0 ? 1.0 : -1.0;
	float stretch = mix(1.0, stretch_along, side_on);
	float foot = mix(0.0, 0.42, side_on);
	vec3 world = origin + right * VERTEX.x * size * mirror + up * (VERTEX.y + foot) * size * stretch;
	VERTEX = (VIEW_MATRIX * vec4(world, 1.0)).xyz;
	vec3 facing = normalize(INV_VIEW_MATRIX[2].xyz);
	NORMAL = normalize(mat3(VIEW_MATRIX) * normalize(mix(facing, vec3(0.0, 1.0, 0.0), sky_normal)));
	vec3 sun = normalize(sun_direction);
	sun_cell = vec3(dot(sun, right * mirror), dot(sun, up), dot(sun, to_camera));
	// The particle's life picks the frame, played once.
	float frame = clamp(floor(INSTANCE_CUSTOM.z * 16.0), 0.0, 15.0);
	float row = floor(variant + 0.5) * 2.0 + floor(frame / 8.0);
	UV = (UV + vec2(mod(frame, 8.0), row)) / vec2(8.0, 6.0);
}

void fragment() {
	vec4 a = texture(flipbook, UV);
	vec4 b = texture(flipbook_b, UV);
	float lit = mix(sixway_ambient(a, b), sixway_light(a, b, normalize(sun_cell)), sun_share);
	ALBEDO = COLOR.rgb * lit * light_gain;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	ALPHA = COLOR.a * a.a;
}
