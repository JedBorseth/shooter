// A billow of a smoke grenade's cloud (or a flashbang's puff, or an
// explosion's dust): a camera-facing particle that plays one of two billowing
// loops of a ball of thick smoke rendered in Blender
// (`assets/textures/effects/smoke_cloud_{a,b}.png`, `tools/build_flipbooks.py`:
// two rows of eight 512-pixel cells per loop, each loop seamless), cross-fading
// between frames so it churns slowly instead of stepping. The flipbook is a
// six-way lightmap (`sixway.gdshaderinc`): the same smoke lit from six sides,
// so each billow's lobes take the sun on the side it shines from and shade
// themselves on the far side. On Low and Medium the scene lights each billow
// per vertex (cheap: the billows are large) and the flipbook shapes that
// light by the sun's direction (`sun_direction`, set by the scripts that
// spawn billows); on High (`SIXWAY_PIXEL`, a copy made by `billows.gd`) each
// light the scene has is applied per pixel through the six-way blend, so a
// muzzle flash or a lamp lights the side of the smoke facing it too. The
// bank is pale in the sun, grey in shade, and never glows at night. It thins out over its lowest
// `ground_fade` metres above the ground it popped on, so the bank has no hard
// line where the billows cross the floor. (A depth-faded edge against walls,
// as High gives gun smoke, is left out: it needs a copy of the depth buffer
// that frames with nobody firing do not otherwise make.)
//
// A whole cloud (the smoke grenade's) can also be shaded as one body: with
// `rounding` above 0, each billow's corners take normals pointing out from
// the cloud's `centre`, so the sun lights the side of the bank facing it and
// its far side and underside fall into shade, instead of every billow facing
// the camera and lighting the same. `variation` gives each billow its own
// brightness and thickness, and `ragged` eats into the billows toward the
// cloud's rim (by their own density, so the edge breaks into wisps rather
// than shrinking as a disc).
shader_type spatial;

// 1 lights every pixel of the billows through the six-way blend (High: a
// copy of this shader with the line rewritten by `billows.gd`; keep its
// form). 0 lights them per vertex.
#define SIXWAY_PIXEL 0
#if SIXWAY_PIXEL
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;
#else
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, vertex_lighting, skip_vertex_transform, diffuse_lambert_wrap;
#endif

#include "res://shaders/sixway.gdshaderinc"

// 1 fades each billow out over its last `SOFT_DEPTH` metres in front of
// whatever is behind it, so a big billow drifting into a wall shows no hard
// line where it cuts in. It reads the depth buffer for every pixel of the
// billows, so only an explosion's column and ring use it, on High, from a
// copy of this shader with the line rewritten (`explosion.gd`'s
// `_soften_billows`); keep the line's form.
#define SOFT_EDGE 0
#if SOFT_EDGE
uniform sampler2D depth_tex : hint_depth_texture, filter_nearest;
const float SOFT_DEPTH = 1.2;
#endif

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform sampler2D flipbook_b : source_color, filter_linear_mipmap, repeat_disable;
uniform vec4 tint : source_color = vec4(0.8, 0.8, 0.79, 1.0);
/// The direction toward the sun (world), for the per-vertex six-way.
uniform vec3 sun_direction = vec3(0.35, 0.8, 0.45);
/// How much of a billow's shading follows the sun (the rest is the even
/// light of the sky), per vertex.
uniform float sun_share = 0.55;
/// The flipbook's light scaled to the old flat billows' brightness.
uniform float light_gain = 2.9;
uniform float opacity = 0.9;
/// Frames a second each billow churns through (16 frames a loop).
uniform float frame_rate = 2.0;
/// Billow size, times the particle's own scale (set per preset).
uniform float size_scale = 1.0;
/// Over this height (metres) above `ground_y` the billows fade in.
uniform float ground_fade = 0.9;
/// How much a billow is lit as part of one round cloud (0: as a flat card
/// facing the camera; 1: by the direction out from `centre`).
uniform float rounding = 0.0;
/// The cloud's radius (metres), for `ragged`.
uniform float cloud_radius = 5.0;
/// How far each billow's brightness and thickness wander (0 to 1).
uniform float variation = 0.0;
/// How much of a billow's thinner parts the rim eats away (0 to 1).
uniform float ragged = 0.0;
/// The ground the cloud popped on (world height), per cloud.
instance uniform float ground_y = -1000.0;
/// The cloud's centre (world), per cloud, for `rounding` and `ragged`.
instance uniform vec3 centre = vec3(0.0, -1000.0, 0.0);

varying float blend;
varying vec2 next_uv;
varying float world_y;
varying float shade;
varying float thick;
varying float cut;
varying vec2 corner_uv;
// The cell's right and up, in view space (the billow is turned by its angle).
varying vec2 cell_x;
varying vec3 sun_cell;
#if SIXWAY_PIXEL
varying vec4 lit_a;
varying vec4 lit_b;
varying float lit_mean;
#endif

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
	vec3 at = (world * vec4(VERTEX, 1.0)).xyz;
	world_y = at.y;
	vec3 facing = normalize(INV_VIEW_MATRIX[2].xyz);
	vec3 normal = facing;
	if (rounding > 0.0) {
		vec3 out_dir = at - centre;
		normal = normalize(mix(facing, normalize(out_dir + facing * 0.001), rounding));
	}
	VERTEX = (VIEW_MATRIX * vec4(at, 1.0)).xyz;
	NORMAL = normalize(mat3(VIEW_MATRIX) * normal);
	// The quad's X runs along (cos a, -sin a) on screen, its Y along
	// (sin a, cos a).
	cell_x = vec2(cos(a), -sin(a));
	vec3 sun_view = normalize(mat3(VIEW_MATRIX) * normalize(sun_direction));
	sun_cell = vec3(dot(sun_view.xy, cell_x), dot(sun_view.xy, vec2(-cell_x.y, cell_x.x)), sun_view.z);
	// Each billow's own brightness and thickness, and how much the rim eats
	// it: none in the middle of the cloud, most at and past its radius.
	float s1 = fract(seed * 13.17);
	float s2 = fract(seed * 29.71);
	shade = 1.0 - variation * 0.35 * s1;
	thick = 1.0 - variation * 0.25 * s2;
	float out_of = length(MODEL_MATRIX[3].xyz - centre) / max(cloud_radius, 0.01);
	cut = ragged * smoothstep(0.6, 1.15, out_of) * (0.6 + 0.4 * s2);
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
	corner_uv = base;
	UV = (base + vec2(mod(frame, 8.0), loop * 2.0 + floor(frame / 8.0))) * cell;
	next_uv = (base + vec2(mod(after, 8.0), loop * 2.0 + floor(after / 8.0))) * cell;
}

void fragment() {
	vec4 tex = texture(flipbook, UV);
	vec4 tex_b = texture(flipbook_b, UV);
	if (blend > 0.0) {
		tex = mix(tex, texture(flipbook, next_uv), blend);
		tex_b = mix(tex_b, texture(flipbook_b, next_uv), blend);
	}
	float mean = sixway_ambient(tex, tex_b);
#if SIXWAY_PIXEL
	// The sky lights the billow by its mean response; light() adds each
	// light through the six-way blend, relative to it.
	lit_a = tex;
	lit_b = tex_b;
	lit_mean = max(mean, 0.02);
	ALBEDO = tint.rgb * mean * light_gain * shade;
#else
	float sun = sixway_light(tex, tex_b, normalize(sun_cell));
	ALBEDO = tint.rgb * mix(mean, sun, sun_share) * light_gain * shade;
#endif
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float ground = clamp((world_y - ground_y) / ground_fade, 0.0, 1.0);
	// The rim keeps only the denser parts of a billow, rescaled so they stay
	// soft at their new edge.
	float density = clamp((tex.a - cut) / max(1.0 - cut, 0.05), 0.0, 1.0);
	// A billow grown large shows its frame's square edge where the cloud
	// reaches it: fade every billow out toward the rim of its quad.
	float rim = 1.0 - smoothstep(0.72, 1.0, length(corner_uv * 2.0 - 1.0));
	ALPHA = min(density * opacity * thick, 1.0) * COLOR.a * ground * rim;
#if SOFT_EDGE
	float raw = texture(depth_tex, SCREEN_UV).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw, 1.0);
	ALPHA *= clamp((VERTEX.z - behind.z / behind.w) / SOFT_DEPTH, 0.0, 1.0);
#endif
}

#if SIXWAY_PIXEL
void light() {
	vec3 l = vec3(dot(LIGHT.xy, cell_x), dot(LIGHT.xy, vec2(-cell_x.y, cell_x.x)), LIGHT.z);
	float lit = sixway_light(lit_a, lit_b, normalize(l)) / lit_mean;
	DIFFUSE_LIGHT += lit * ATTENUATION * LIGHT_COLOR / PI;
}
#endif
