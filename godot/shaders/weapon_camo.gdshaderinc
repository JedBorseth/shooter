// A weapon camo (docs/progression.md), painted in the weapon's own material
// so it costs no extra draw: first person (viewmodel.gdshader) and third
// person and killcam (weapon_camo.gdshader). Off until `camo_on`.
//
// The pattern (tools/build_camos.py) is projected onto the gun from three
// sides in its own space, so it needs no UVs, and covers the coated parts:
// those the weapon's ORM map does not mark as bare metal. Edges worn to bare
// metal stay bare, and the base colour's grime and occlusion darken the
// paint, as Call of Duty's camos sit under the weapon's wear.
//
// `camo_style`: 0 a painted pattern, 1 metal (RGB is the metal), 2 facets
// whose hue turns with the view (R hue seed, G brightness), 3 drifting
// clouds with glinting stars (RGB clouds, A stars).

uniform bool camo_on = false;
uniform sampler2D camo_texture : source_color, filter_linear_mipmap, repeat_enable, hint_default_white;
// Pattern repeats a metre.
uniform float camo_scale = 4.0;
uniform float camo_metallic = 0.1;
uniform float camo_roughness = 0.6;
uniform int camo_style = 0;

varying vec3 camo_position;
varying vec3 camo_normal;

// In vertex(): the gun-space position and normal the pattern follows
// (varyings can only be written there).
#define CAMO_VERTEX camo_position = VERTEX; camo_normal = NORMAL;

vec4 camo_sample(vec3 offset) {
	vec3 w = pow(abs(normalize(camo_normal)), vec3(4.0));
	w /= max(w.x + w.y + w.z, 1e-4);
	vec3 p = camo_position * camo_scale + offset;
	return texture(camo_texture, p.zy) * w.x + texture(camo_texture, p.xz) * w.y
			+ texture(camo_texture, p.xy) * w.z;
}

// Paints the camo over `albedo`, `roughness`, and `metallic` where the
// weapon's source metallic (`source_metallic`, from its ORM map) marks a
// coat. `normal` and `view` are in view space.
void camo_apply(inout vec3 albedo, inout float roughness, inout float metallic,
		inout vec3 emission, float source_metallic, vec3 normal, vec3 view, float time) {
	if (!camo_on) {
		return;
	}
	float coat = 1.0 - smoothstep(0.45, 0.65, source_metallic);
	if (coat <= 0.0) {
		return;
	}
	vec4 c = camo_sample(vec3(0.0));
	// The base colour of a coated part is about 0.14; its grime and
	// occlusion carry over as darker paint.
	float lum = dot(albedo, vec3(0.2126, 0.7152, 0.0722));
	float detail = clamp(lum / 0.14, 0.45, 1.4);
	vec3 paint = c.rgb;
	if (camo_style == 2) {
		float facing = dot(normal, view);
		vec3 hue = 0.5 + 0.5 * cos(6.2831853 * (c.r + facing * 0.7 + vec3(0.0, 0.33, 0.67)));
		paint = mix(vec3(0.85), hue, 0.42) * (0.45 + 0.55 * c.g);
	} else if (camo_style == 3) {
		vec4 drift = camo_sample(vec3(time * 0.015, 0.0, time * 0.01));
		paint = mix(c.rgb, drift.rgb, 0.5);
		float twinkle = 0.5 + 0.5 * sin(time * 3.0 + c.a * 40.0);
		emission += vec3(0.75, 0.8, 1.0) * c.a * twinkle * 1.5 * coat;
	}
	paint *= mix(1.0, detail, 0.6);
	albedo = mix(albedo, paint, coat);
	roughness = mix(roughness, camo_roughness, coat);
	metallic = mix(metallic, camo_metallic, coat);
}
