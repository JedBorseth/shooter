// Chips knocked off a surface by a bullet: thrown out in a cone along the
// emitter's +Y (the surface normal, or the round's travel at an exit), they
// tumble, fall, bounce once or twice on the floor below (`floor_y`, found by
// a ray when the impact plays), and come to rest there instead of falling
// through it, then shrink away at the end of their life. Each chip gets its
// own shape (a different stretch on each axis of the one chip mesh) and its
// own shade of `base_color`, the colour of what was hit (COLOR, for a
// material that takes the vertex colour as albedo).
shader_type particles;
render_mode disable_force;

uniform float spread_degrees = 50.0;
uniform float speed_min = 1.5;
uniform float speed_max = 4.5;
uniform float gravity = 12.0;
uniform float scale_min = 0.5;
uniform float scale_max = 1.3;
/// World height of the floor below the impact (far below when there is none).
uniform float floor_y = -1000.0;
/// Share of the speed kept across the floor on a bounce, and along it.
uniform float bounce = 0.3;
uniform float friction = 0.55;
/// The chips' colour, and how far each one's brightness wanders from it.
uniform vec4 base_color : source_color = vec4(0.5, 0.5, 0.5, 1.0);
uniform float color_jitter = 0.35;

float hash(uint x) {
	x ^= x >> 16u;
	x *= 0x7feb352du;
	x ^= x >> 15u;
	x *= 0x846ca68bu;
	x ^= x >> 16u;
	return float(x) / 4294967295.0;
}

mat3 rotation(vec3 axis, float angle) {
	float s = sin(angle);
	float c = cos(angle);
	float t = 1.0 - c;
	return mat3(
		vec3(t * axis.x * axis.x + c, t * axis.x * axis.y + s * axis.z, t * axis.x * axis.z - s * axis.y),
		vec3(t * axis.x * axis.y - s * axis.z, t * axis.y * axis.y + c, t * axis.y * axis.z + s * axis.x),
		vec3(t * axis.x * axis.z + s * axis.y, t * axis.y * axis.z - s * axis.x, t * axis.z * axis.z + c)
	);
}

void start() {
	uint seed = NUMBER * 747796405u + RANDOM_SEED;
	float r0 = hash(seed);
	float r1 = hash(seed + 1u);
	float r2 = hash(seed + 2u);
	float r3 = hash(seed + 3u);
	float r4 = hash(seed + 4u);
	if (RESTART_POSITION) {
		TRANSFORM = mat4(vec4(1, 0, 0, 0), vec4(0, 1, 0, 0), vec4(0, 0, 1, 0), EMISSION_TRANSFORM[3]);
	}
	if (RESTART_VELOCITY) {
		// A direction inside the cone round local +Y, in world space.
		float cone = radians(spread_degrees) * sqrt(r0);
		float around = r1 * TAU;
		vec3 local = vec3(sin(cone) * cos(around), cos(cone), sin(cone) * sin(around));
		vec3 direction = normalize(mat3(EMISSION_TRANSFORM) * local);
		VELOCITY = direction * mix(speed_min, speed_max, r2);
	}
	// CUSTOM: x spin rate (radians a second), y angle so far, z scale, w resting.
	CUSTOM = vec4((r3 * 2.0 - 1.0) * 12.0, r4 * TAU, mix(scale_min, scale_max, r3 * 0.5 + r4 * 0.5), 0.0);
	// USERDATA1.x: seconds since it was thrown; .yzw its stretch per axis.
	float r5 = hash(seed + 8u);
	float r6 = hash(seed + 9u);
	float r7 = hash(seed + 10u);
	USERDATA1 = vec4(0.0, 0.7 + 0.6 * r5, 0.6 + 0.6 * r6, 0.7 + 0.6 * r7);
	float shade = 1.0 + (r5 * 0.5 + r7 * 0.5 - 0.5) * 2.0 * color_jitter;
	COLOR = vec4(clamp(base_color.rgb * shade, vec3(0.0), vec3(1.0)), 1.0);
}

void process() {
	uint seed = NUMBER * 747796405u + RANDOM_SEED;
	vec3 axis = normalize(vec3(hash(seed + 5u) - 0.5, hash(seed + 6u) - 0.5, hash(seed + 7u) - 0.5) + vec3(0.0, 0.0, 0.001));
	vec3 position = TRANSFORM[3].xyz;
	USERDATA1.x += DELTA;
	if (USERDATA1.x > LIFETIME) {
		ACTIVE = false;
	}
	if (CUSTOM.w < 0.5) {
		VELOCITY.y -= gravity * DELTA;
		CUSTOM.y += CUSTOM.x * DELTA;
		vec3 next = position + VELOCITY * DELTA;
		if (next.y < floor_y && VELOCITY.y < 0.0) {
			VELOCITY.y = -VELOCITY.y * bounce;
			VELOCITY.xz *= friction;
			CUSTOM.x *= 0.5;
			if (VELOCITY.y < 0.6) {
				// Too slow to bounce again: lie on the floor.
				VELOCITY = vec3(0.0);
				CUSTOM.w = 1.0;
				position.y = floor_y + 0.004;
			}
		}
	}
	// Shrink away over the last fifth of the life.
	float fade = clamp((LIFETIME - USERDATA1.x) / (0.2 * LIFETIME), 0.0, 1.0);
	mat3 basis = rotation(axis, CUSTOM.y) * mat3(
		vec3(USERDATA1.y, 0.0, 0.0), vec3(0.0, USERDATA1.z, 0.0), vec3(0.0, 0.0, USERDATA1.w))
		* (CUSTOM.z * fade);
	TRANSFORM = mat4(vec4(basis[0], 0.0), vec4(basis[1], 0.0), vec4(basis[2], 0.0), vec4(position, 1.0));
}
