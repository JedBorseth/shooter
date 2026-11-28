// A bullet impact's dust puff (or a hit's blood mist): a camera-facing
// particle playing a puff rendered in Blender
// (`assets/textures/effects/impact_puff_{a,b}.png`, `tools/build_flipbooks.py`:
// 256-pixel cells, three variants of 16 frames, two atlas rows each: fine
// dust off concrete, plaster and wood, a clumpier dirt burst, and a blood
// mist; `variant` picks one per impact), once over the particle's life, in
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
/// Which puff: 0 dust, 1 dirt, 2 blood mist.
instance uniform float variant = 0.0;

varying vec3 sun_cell;

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
	// The cell's right on screen is (cos a, sin a), its up (-sin a, cos a).
	vec2 cell_x = vec2(cos(a), sin(a));
	vec3 sun_view = normalize(mat3(VIEW_MATRIX) * normalize(sun_direction));
	sun_cell = vec3(dot(sun_view.xy, cell_x), dot(sun_view.xy, vec2(-cell_x.y, cell_x.x)), sun_view.z);
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
