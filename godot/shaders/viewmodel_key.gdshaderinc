// The first-person key light (scripts/viewmodel_light.gd), shared by the
// first-person weapon (viewmodel.gdshader) and the first-person arms (a copy
// of character.gdshader compiled with VIEWMODEL_KEY, made in
// rust/game/src/viewmodel.rs: `first_person_shader`). It is added as emission
// on the first-person pixels only, rather than as a light node, which would
// cost every lit pixel of the world.

// The direction toward the key in world space and its strength, and its
// colour. The colour's alpha is how much of the key is the sky rather than
// the sun (0 to 1): a sky lights from a whole dome, so it wraps round the
// weapon further than the sun does.
global uniform vec4 viewmodel_key;
global uniform vec4 viewmodel_key_color;

// How far the sky's light wraps past the weapon's side faces (0 is plain
// Lambert), and how much of the baked occlusion the key takes: the ORM's
// occlusion is a 30 mm bake that blackens every crevice, which suits a
// point light but not a sky.
const float VIEWMODEL_SKY_WRAP = 0.6;
const float VIEWMODEL_KEY_OCCLUSION = 0.6;
// The sky's reflection along the weapon's top faces and edges, as a share of
// the key: a black rifle in shade reads by the sheen of the sky on it, as a
// Call of Duty viewmodel does, not by its diffuse colour.
const float VIEWMODEL_SHEEN = 0.7;

// The key's light on a surface: a shadowless Lambert diffuse (wrapped for the
// sky), a normalised Blinn-Phong lobe, and a Fresnel sheen of the sky over
// faces that reflect it, in the renderer's units (a light of energy E lights
// white paper facing it to E). `normal` and `view` are in view space.
vec3 viewmodel_key_light(
		vec3 normal, vec3 view, mat4 view_matrix, vec3 albedo, float roughness,
		float metallic, float occlusion) {
	vec3 l = normalize((view_matrix * vec4(viewmodel_key.xyz, 0.0)).xyz);
	float wrap = VIEWMODEL_SKY_WRAP * viewmodel_key_color.a;
	float ndl = clamp((dot(normal, l) + wrap) / (1.0 + wrap), 0.0, 1.0);
	float shine = exp2(11.0 * (1.0 - roughness) + 1.0);
	float lobe = pow(max(dot(normal, normalize(l + view)), 0.0), shine) * (shine + 8.0) / 25.13;
	vec3 f0 = mix(vec3(0.04), albedo, metallic);
	float occ = mix(1.0, occlusion, VIEWMODEL_KEY_OCCLUSION);
	vec3 lit = (albedo * (1.0 - metallic) + f0 * lobe * 0.25) * ndl;
	// The sheen: Schlick's Fresnel toward the sky's colour, on faces whose
	// reflection looks up into the sky. Rough faces blur it and dim its edge.
	vec3 up = normalize((view_matrix * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
	vec3 r = reflect(-view, normal);
	float skyward = smoothstep(-0.25, 0.75, dot(r, up));
	float nv = clamp(dot(normal, view), 0.0, 1.0);
	float gloss = 1.0 - roughness;
	vec3 edge = max(vec3(gloss), f0);
	vec3 fresnel = f0 + (edge - f0) * pow(1.0 - nv, 5.0);
	vec3 sheen = fresnel * skyward * (0.3 + 0.7 * gloss * gloss) * VIEWMODEL_SHEEN;
	return (lit + sheen) * occ * viewmodel_key_color.rgb * viewmodel_key.w;
}

// The tangent-space normal map's normal in view space, as the renderer builds
// it from NORMAL_MAP, for the key.
vec3 viewmodel_key_normal(vec3 normal_map, float depth, vec3 normal, vec3 tangent, vec3 binormal) {
	vec3 nm;
	nm.xy = (normal_map.xy * 2.0 - 1.0) * depth;
	nm.z = sqrt(max(1.0 - dot(nm.xy, nm.xy), 0.0));
	return normalize(tangent * nm.x + binormal * nm.y + normal * nm.z);
}
