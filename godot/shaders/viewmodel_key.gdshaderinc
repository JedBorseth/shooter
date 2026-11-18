// The first-person key light (scripts/viewmodel_light.gd), shared by the
// first-person weapon (viewmodel.gdshader) and the first-person arms (a copy
// of character.gdshader compiled with VIEWMODEL_KEY, made in
// rust/game/src/viewmodel.rs: `first_person_shader`). It is added as emission
// on the first-person pixels only, rather than as a light node, which would
// cost every lit pixel of the world.

// The direction toward the key in world space and its strength, and its colour.
global uniform vec4 viewmodel_key;
global uniform vec4 viewmodel_key_color;

// The key's light on a surface: a shadowless Lambert diffuse and a normalised
// Blinn-Phong lobe, in the renderer's units (a light of energy E lights white
// paper facing it to E). `normal` and `view` are in view space.
vec3 viewmodel_key_light(
		vec3 normal, vec3 view, mat4 view_matrix, vec3 albedo, float roughness,
		float metallic, float occlusion) {
	vec3 l = normalize((view_matrix * vec4(viewmodel_key.xyz, 0.0)).xyz);
	float ndl = max(dot(normal, l), 0.0);
	float shine = exp2(11.0 * (1.0 - roughness) + 1.0);
	float lobe = pow(max(dot(normal, normalize(l + view)), 0.0), shine) * (shine + 8.0) / 25.13;
	vec3 f0 = mix(vec3(0.04), albedo, metallic);
	vec3 lit = albedo * (1.0 - metallic) + f0 * lobe * 0.25;
	return lit * ndl * occlusion * viewmodel_key_color.rgb * viewmodel_key.w;
}

// The tangent-space normal map's normal in view space, as the renderer builds
// it from NORMAL_MAP, for the key.
vec3 viewmodel_key_normal(vec3 normal_map, float depth, vec3 normal, vec3 tangent, vec3 binormal) {
	vec3 nm;
	nm.xy = (normal_map.xy * 2.0 - 1.0) * depth;
	nm.z = sqrt(max(1.0 - dot(nm.xy, nm.xy), 0.0));
	return normalize(tangent * nm.x + binormal * nm.y + normal * nm.z);
}
