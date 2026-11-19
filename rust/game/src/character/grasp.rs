//! Fitting a weapon's finger poses to its model.
//!
//! Shooters author the hands on every weapon by hand: Call of Duty's and
//! Halo's first-person animations are made per weapon, with the fingers
//! posed on that weapon's grip, and VR toolkits put an authored hand pose on
//! each grab point. This game does the same, a pose per weapon in its weapon
//! file (`WeaponDef::hands`), but fits those poses to the model instead of
//! posing them by eye: a small distance field around each grip ([`Field`],
//! cut from the meshes the weapon shows), and each finger's joints chosen
//! ([`fit_finger`]) so no part of the gloved finger is inside the weapon,
//! its pads rest on the surface where they reach it, and it stays close to
//! the grip's shape (`HandShape` in `super::poser`). The fit runs offline
//! (`godot/tests/bake_hand_poses.gd`); the game only reads the result.

use godot::classes::{MeshInstance3D, Node3D};
use godot::prelude::*;

/// Distance field resolution (m between samples) and how far from the
/// surface it measures: past `BAND` everything reads as `BAND`.
const CELL: f32 = 0.004;
const BAND: f32 = 0.016;

/// Distance to the weapon's surface on a regular grid, in the weapon's
/// space, trilinearly sampled: negative inside a closed part of the weapon
/// (a receiver, a slide), so a finger buried deep in one reads as inside
/// rather than as far from its surface. Open parts (a plate modelled as a
/// single sheet) have no inside, and only their distance counts.
#[derive(Clone, Debug)]
pub struct Field {
    origin: Vector3,
    dims: [usize; 3],
    dist: Vec<f32>,
}

impl Field {
    /// The field of `triangles` over the box `centre ± half`, or `None` if no
    /// triangle comes within the band of it.
    pub fn from_triangles(triangles: &[Tri], centre: Vector3, half: Vector3) -> Option<Self> {
        let origin = centre - half;
        let count = |h: f32| ((2.0 * h) / CELL).ceil() as usize + 1;
        let dims = [count(half.x), count(half.y), count(half.z)];
        let far = BAND * BAND;
        let mut dist = vec![far; dims[0] * dims[1] * dims[2]];
        // Which side of the nearest surface each sample is on: 1 outside,
        // -1 inside a closed part, 0 not yet known (past the band).
        let mut side = vec![0i8; dist.len()];
        let lo = origin - Vector3::splat(BAND);
        let hi = centre + half + Vector3::splat(BAND);
        let mut any = false;
        for triangle in triangles {
            let tri = &triangle.corners;
            let tmin = tri[0].coord_min(tri[1]).coord_min(tri[2]);
            let tmax = tri[0].coord_max(tri[1]).coord_max(tri[2]);
            if tmax.x < lo.x || tmax.y < lo.y || tmax.z < lo.z {
                continue;
            }
            if tmin.x > hi.x || tmin.y > hi.y || tmin.z > hi.z {
                continue;
            }
            let range = |axis: usize, min: f32, max: f32| {
                let o = [origin.x, origin.y, origin.z][axis];
                let a = ((min - BAND - o) / CELL).floor().max(0.0) as usize;
                let b = (((max + BAND - o) / CELL).ceil().max(0.0) as usize).min(dims[axis] - 1);
                (a, b)
            };
            let (x0, x1) = range(0, tmin.x, tmax.x);
            let (y0, y1) = range(1, tmin.y, tmax.y);
            let (z0, z1) = range(2, tmin.z, tmax.z);
            if x0 > x1 || y0 > y1 || z0 > z1 {
                continue;
            }
            for z in z0..=z1 {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let p = origin + Vector3::new(x as f32, y as f32, z as f32) * CELL;
                        let (q, feature) = closest_on_triangle(p, tri);
                        let d = (p - q).length_squared();
                        let i = (z * dims[1] + y) * dims[0] + x;
                        if d < dist[i] {
                            dist[i] = d;
                            let inside = triangle
                                .normals
                                .is_some_and(|n| (p - q).dot(n.at(feature)) < 0.0);
                            side[i] = if inside { -1 } else { 1 };
                            any = true;
                        }
                    }
                }
            }
        }
        if !any {
            return None;
        }
        // Past the band, a sample takes the side of the nearest sample that
        // knows its side (deep inside a thick part is still inside).
        let mut queue: std::collections::VecDeque<usize> =
            (0..side.len()).filter(|i| side[*i] != 0).collect();
        while let Some(i) = queue.pop_front() {
            let (x, y, z) = (
                i % dims[0],
                (i / dims[0]) % dims[1],
                i / (dims[0] * dims[1]),
            );
            let mut visit = |j: usize| {
                if side[j] == 0 {
                    side[j] = side[i];
                    queue.push_back(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < dims[0] {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - dims[0]);
            }
            if y + 1 < dims[1] {
                visit(i + dims[0]);
            }
            if z > 0 {
                visit(i - dims[0] * dims[1]);
            }
            if z + 1 < dims[2] {
                visit(i + dims[0] * dims[1]);
            }
        }
        for (d, s) in dist.iter_mut().zip(&side) {
            *d = d.sqrt() * if *s < 0 { -1.0 } else { 1.0 };
        }
        Some(Self { origin, dims, dist })
    }

    /// Distance from `p` (weapon space) to the surface, at most `BAND`,
    /// negative inside a closed part.
    pub fn distance(&self, p: Vector3) -> f32 {
        let g = (p - self.origin) / CELL;
        let (fx, fy, fz) = (g.x.floor(), g.y.floor(), g.z.floor());
        if fx < 0.0 || fy < 0.0 || fz < 0.0 {
            return BAND;
        }
        let (x, y, z) = (fx as usize, fy as usize, fz as usize);
        if x + 1 >= self.dims[0] || y + 1 >= self.dims[1] || z + 1 >= self.dims[2] {
            return BAND;
        }
        let (tx, ty, tz) = (g.x - fx, g.y - fy, g.z - fz);
        let at =
            |x: usize, y: usize, z: usize| self.dist[(z * self.dims[1] + y) * self.dims[0] + x];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let plane = |z: usize| {
            lerp(
                lerp(at(x, y, z), at(x + 1, y, z), tx),
                lerp(at(x, y + 1, z), at(x + 1, y + 1, z), tx),
                ty,
            )
        };
        lerp(plane(z), plane(z + 1), tz)
    }
}

/// The part of a triangle a nearest point lies on: a corner, an edge (edge
/// `i` runs from corner `i` to corner `i + 1`), or the face between.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Feature {
    Corner(usize),
    Edge(usize),
    Face,
}

/// The point of a triangle nearest `p`, and the part of the triangle it lies
/// on (Ericson, Real-Time Collision Detection, 5.1.5).
fn closest_on_triangle(p: Vector3, [a, b, c]: &[Vector3; 3]) -> (Vector3, Feature) {
    let (ab, ac, ap) = (*b - *a, *c - *a, p - *a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (*a, Feature::Corner(0));
    }
    let bp = p - *b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (*b, Feature::Corner(1));
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (*a + ab * (d1 / (d1 - d3)), Feature::Edge(0));
    }
    let cp = p - *c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (*c, Feature::Corner(2));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return (*a + ac * (d2 / (d2 - d6)), Feature::Edge(2));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (*b + (*c - *b) * t, Feature::Edge(1));
    }
    let denom = 1.0 / (va + vb + vc);
    (*a + ab * (vb * denom) + ac * (vc * denom), Feature::Face)
}

/// A triangle of a weapon's surface, in the weapon's space.
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub corners: [Vector3; 3],
    /// For a triangle of a closed part, which way is out at each of its
    /// parts; none for an open part, which has no inside.
    normals: Option<Normals>,
}

/// Which way is out at each part of a triangle of a closed part: the
/// angle-weighted pseudo-normals of Bærentzen and Aanæs ("Signed distance
/// computation using the angle weighted pseudonormal", 2005), so the
/// triangles that share an edge or a corner agree on which side of it a
/// point is, and the sign of a distance does not hang on which of them is
/// nearest by a hair.
#[derive(Clone, Copy, Debug)]
struct Normals {
    face: Vector3,
    edges: [Vector3; 3],
    corners: [Vector3; 3],
}

impl Normals {
    fn at(&self, feature: Feature) -> Vector3 {
        match feature {
            Feature::Corner(i) => self.corners[i],
            Feature::Edge(i) => self.edges[i],
            Feature::Face => self.face,
        }
    }
}

impl Tri {
    /// A triangle of an open part (a single sheet), which has no inside.
    pub fn open(corners: [Vector3; 3]) -> Self {
        Self {
            corners,
            normals: None,
        }
    }

    /// The triangles of a closed part, wound with their fronts outward,
    /// with the normals that say which side of them is out.
    fn closed(tris: &[[Vector3; 3]]) -> Vec<Self> {
        use std::collections::HashMap;
        let face = |t: &[Vector3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).normalized_or_zero();
        let mut edges: HashMap<(Key, Key), Vector3> = HashMap::new();
        let mut corners: HashMap<Key, Vector3> = HashMap::new();
        for t in tris {
            let n = face(t);
            for i in 0..3 {
                let (a, b, c) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                *edges.entry(edge_key(a, b)).or_insert(Vector3::ZERO) += n;
                // The corner's share: its angle in this triangle.
                let angle = (b - a).angle_to(c - a);
                *corners.entry(key(a)).or_insert(Vector3::ZERO) += n * angle;
            }
        }
        tris.iter()
            .map(|t| Self {
                corners: *t,
                normals: Some(Normals {
                    face: face(t),
                    edges: std::array::from_fn(|i| edges[&edge_key(t[i], t[(i + 1) % 3])]),
                    corners: std::array::from_fn(|i| corners[&key(t[i])]),
                }),
            })
            .collect()
    }
}

/// A corner's position to a hundredth of a millimetre, so the copies of a
/// corner a mesh splits at UV seams and hard edges count as one.
type Key = (i64, i64, i64);
fn key(p: Vector3) -> Key {
    let q = |v: f32| (v * 1.0e5).round() as i64;
    (q(p.x), q(p.y), q(p.z))
}
fn edge_key(a: Vector3, b: Vector3) -> (Key, Key) {
    let (a, b) = (key(a), key(b));
    if a < b { (a, b) } else { (b, a) }
}

/// The fields around a weapon's two grips, measured at their rest markers.
#[derive(Clone, Debug)]
pub struct WeaponFields {
    pub right: Option<Field>,
    pub left: Option<Field>,
}

/// How far each field reaches from its grip marker: a hand's width across, and
/// a finger's reach up, down, and along the weapon.
const RIGHT_HALF: Vector3 = Vector3::new(0.07, 0.09, 0.09);
const LEFT_HALF: Vector3 = Vector3::new(0.075, 0.085, 0.09);

impl WeaponFields {
    /// Cuts both fields from every mesh `view` shows, in its own space.
    pub fn measure(view: &Gd<Node3D>, right_rest: Vector3, left_rest: Vector3) -> Self {
        let triangles = shown_triangles(view);
        Self {
            right: Field::from_triangles(&triangles, right_rest, RIGHT_HALF),
            left: Field::from_triangles(&triangles, left_rest, LEFT_HALF),
        }
    }
}

/// Every triangle of the meshes `view` shows (it and every parent up to it
/// set visible), in the view's space to a hundredth of a millimetre. A
/// closed mesh (every edge shared by exactly two triangles) is wound with
/// its fronts outward (a positive volume), whatever its import's winding,
/// and knows its inside; an open one does not.
pub fn shown_triangles(view: &Gd<Node3D>) -> Vec<Tri> {
    let inverse = view.get_global_transform().affine_inverse();
    let mut out = Vec::new();
    for node in view
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        let Ok(mesh_node) = node.try_cast::<MeshInstance3D>() else {
            continue;
        };
        if !super::rig::shown_under(&mesh_node.clone().upcast(), view) {
            continue;
        }
        let Some(mesh) = mesh_node.get_mesh() else {
            continue;
        };
        let to_view = inverse * mesh_node.get_global_transform();
        // Rounded, so a hair's difference in where the view sits does not
        // change the fit.
        let round = |p: Vector3| (p * 1.0e5).round() / 1.0e5;
        let faces = mesh.get_faces();
        let tris: Vec<[Vector3; 3]> = faces
            .as_slice()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|p| round(to_view * p)))
            .collect();
        if is_closed(&tris) {
            let volume: f32 = tris.iter().map(|[a, b, c]| a.dot(b.cross(*c))).sum();
            let outward: Vec<_> = if volume < 0.0 {
                tris.iter().map(|[a, b, c]| [*a, *c, *b]).collect()
            } else {
                tris
            };
            out.extend(Tri::closed(&outward));
        } else {
            out.extend(tris.into_iter().map(Tri::open));
        }
    }
    out
}

/// Whether every edge of `tris` is shared by exactly two of them.
fn is_closed(tris: &[[Vector3; 3]]) -> bool {
    if tris.is_empty() {
        return false;
    }
    let mut edges: std::collections::HashMap<_, u32> = std::collections::HashMap::new();
    for t in tris {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            if key(a) == key(b) {
                continue;
            }
            *edges.entry(edge_key(a, b)).or_default() += 1;
        }
    }
    edges.values().all(|n| *n == 2)
}

/// One finger's bones in its hand's space: each joint's offset from its
/// parent (the hand, then the previous joint) and its rest rotation, and how
/// far the fingertip is past the last joint.
#[derive(Clone, Copy, Debug)]
pub struct Chain {
    pub offsets: [Vector3; 3],
    pub rests: [Quaternion; 3],
    pub tip: f32,
    pub thumb: bool,
}

/// The gloved finger around each bone (hand-bone units), measured from the
/// first-person gloves (`arms.glb`): a finger's bone runs near its back, so
/// the flesh and the pads sit toward the palm (+Z).
struct Segment {
    radius: f32,
    offset: Vector3,
}
const FINGER: [Segment; 3] = [
    Segment {
        radius: 0.0125,
        offset: Vector3::new(0.0, 0.0, 0.005),
    },
    Segment {
        radius: 0.0115,
        offset: Vector3::new(0.0, 0.0, 0.005),
    },
    Segment {
        radius: 0.0100,
        offset: Vector3::new(0.0, 0.0, 0.003),
    },
];
const THUMB: [Segment; 3] = [
    // The metacarpal is inside the ball of the thumb, which the palm's own
    // placement keeps off the grip.
    Segment {
        radius: 0.0,
        offset: Vector3::ZERO,
    },
    Segment {
        radius: 0.014,
        offset: Vector3::new(0.008, 0.0, 0.006),
    },
    Segment {
        radius: 0.012,
        offset: Vector3::new(0.008, 0.0, 0.0065),
    },
];
/// Where along each segment the finger is tested (share of its length):
/// close enough together that a rail's edge cannot slip between two tests.
const SAMPLES: [&[f32]; 3] = [&[0.2, 0.45, 0.7, 1.0], &[0.35, 0.7, 1.0], &[0.3, 0.6, 1.0]];
const SAMPLE_COUNT: usize = 10;
/// A glove gives a little: a finger may press this share of its radius
/// into the surface.
const PRESS: f32 = 0.9;

/// Where a hand is relative to the field it closes on.
#[derive(Clone, Copy, Debug)]
pub struct Contact<'a> {
    pub field: &'a Field,
    /// From the hand bone's space to the field's (weapon space).
    pub hand_to_field: Transform3D,
    /// The left hand mirrors the thumb's sideways offset.
    pub left: bool,
}

impl Contact<'_> {
    fn scale(&self) -> f32 {
        self.hand_to_field.basis.col_a().length()
    }
}

/// Each tested point of a finger posed at `flex` and `spread`, in the
/// field's space, with the finger's radius there (m): `None` for points the
/// finger does not test.
fn sample_points(
    chain: &Chain,
    contact: &Contact,
    flex: [f32; 3],
    spread: f32,
    joint: &impl Fn(usize, f32, f32) -> Quaternion,
) -> [Option<(Vector3, f32)>; SAMPLE_COUNT] {
    let segments = if chain.thumb { &THUMB } else { &FINGER };
    let scale = contact.scale();
    let side = if contact.left { -1.0 } else { 1.0 };
    let mut out = [None; SAMPLE_COUNT];
    let mut frame = Transform3D::IDENTITY;
    let mut n = 0;
    for (s, segment) in segments.iter().enumerate() {
        let rotation = chain.rests[s] * joint(s, flex[s], spread);
        frame = frame * Transform3D::new(Basis::from_quaternion(rotation), chain.offsets[s]);
        let length = if s < 2 {
            chain.offsets[s + 1].length()
        } else {
            // The capsule's end is a radius short of the fingertip.
            (chain.tip - segment.radius).max(0.0)
        };
        let offset = Vector3::new(segment.offset.x * side, segment.offset.y, segment.offset.z);
        for &t in SAMPLES[s] {
            if segment.radius > 0.0 {
                let p = contact.hand_to_field * (frame * (Vector3::UP * (length * t) + offset));
                out[n] = Some((p, segment.radius * scale));
            }
            n += 1;
        }
    }
    out
}

/// How far each tested point of a finger posed at `flex` is into the
/// surface (m; negative: clear of it). Points the finger does not test read
/// `f32::MIN`.
fn depths(
    chain: &Chain,
    contact: &Contact,
    flex: [f32; 3],
    spread: f32,
    joint: &impl Fn(usize, f32, f32) -> Quaternion,
) -> [f32; SAMPLE_COUNT] {
    sample_points(chain, contact, flex, spread, joint)
        .map(|p| p.map_or(f32::MIN, |(p, r)| r * PRESS - contact.field.distance(p)))
}

/// The points along a finger posed at `flex` and `spread` (the field's
/// space) with the finger's radius at each, for offline tools that check
/// what a pose shows (a thumb rising into a sight's window).
pub fn finger_points(
    chain: &Chain,
    contact: &Contact,
    flex: [f32; 3],
    spread: f32,
    joint: impl Fn(usize, f32, f32) -> Quaternion,
) -> Vec<(Vector3, f32)> {
    sample_points(chain, contact, flex, spread, &joint)
        .into_iter()
        .flatten()
        .collect()
}

/// The deepest any part of a finger posed at `flex` and `spread` goes into
/// the surface (m; negative: it is clear by that much). `joint(j, flex,
/// spread)` is the rotation joint `j` takes on top of its rest.
pub fn finger_depth(
    chain: &Chain,
    contact: &Contact,
    flex: [f32; 3],
    spread: f32,
    joint: impl Fn(usize, f32, f32) -> Quaternion,
) -> f32 {
    depths(chain, contact, flex, spread, &joint)
        .into_iter()
        .fold(f32::MIN, f32::max)
}

/// Closing a finger: from this share of the grip's curl toward
/// `CLOSE` times it plus `CLOSE_EXTRA` (degrees per joint, at most
/// `CLOSE_MOST`), in `STEPS` steps then halvings of the step it touched in,
/// so a finger can open on a thick handguard and close round a thin grip.
const OPEN: f32 = 0.15;
const CLOSE: f32 = 1.3;
const CLOSE_EXTRA: [f32; 3] = [8.0, 8.0, 6.0];
/// A thumb's shape is nearly straight, laid along what the hand holds: it
/// closes further, so it comes to rest on the weapon's side instead of in the
/// air beside it.
const THUMB_CLOSE_EXTRA: [f32; 3] = [25.0, 25.0, 20.0];
const CLOSE_MOST: [f32; 3] = [95.0, 105.0, 80.0];
const STEPS: usize = 18;
const REFINE: usize = 3;
/// Clearing what is left: how far (degrees) the curl and the knuckle's turn
/// may move from where the finger closed (wider for the trigger finger, which
/// threads the guard), what being inside costs (per m², past a tolerance),
/// and what each degree moved costs, so it moves only as far as clearing the
/// surface needs.
const CLEAR_CURL: f32 = 12.0;
const CLEAR_TURN: f32 = 6.0;
const CLEAR_WIDE: f32 = 2.0;
/// A finger still this far inside (m) after a nudge tries a wider one, up to
/// three times as wide.
const CLEAR_ENOUGH: f32 = 0.001;
const CLEAR_WIDEN: [f32; 3] = [1.0, 2.0, 3.0];
const INSIDE: f32 = 1.0e8;
const TOLERANCE: f32 = 0.0003;
const MOVED: f32 = 1.0e-3;

/// A finger's curl (degrees per joint) and its turn at the knuckle (degrees
/// of spread, or the thumb's turn across the palm), fitted to the surface
/// from the grip's shape, as a hand closes on what it holds: every joint
/// closes together from nearly straight, each stopping where the finger
/// beyond it meets the surface (a finger that meets nothing keeps the shape),
/// then the pose is nudged (`CLEAR_CURL`, `CLEAR_TURN`, wider if that is not
/// enough) to clear anything still inside. `close` false skips the closing
/// (the trigger finger laid along the frame keeps its shape and only clears
/// the surface); `wide` widens the nudge; `turn` false keeps the spread.
/// `joint(j, flex, spread)` is joint `j`'s rotation on top
/// of its rest.
pub fn fit_finger(
    chain: &Chain,
    contact: &Contact,
    (shape, spread): ([f32; 3], f32),
    (close, wide, turn): (bool, bool, bool),
    joint: impl Fn(usize, f32, f32) -> Quaternion,
) -> ([f32; 3], f32) {
    let closed = if close {
        close_finger(chain, contact, shape, spread, &joint).unwrap_or(shape)
    } else {
        shape
    };
    // Clear what is still inside, moving as little as it can, and further
    // only if a small nudge leaves it inside.
    let mut best = (closed, spread);
    for widen in CLEAR_WIDEN {
        let reach = widen * if wide { CLEAR_WIDE } else { 1.0 };
        best = clear(chain, contact, (closed, spread), (reach, turn), &joint);
        let depth = finger_depth(chain, contact, best.0, best.1, &joint);
        if depth <= CLEAR_ENOUGH {
            break;
        }
    }
    let half = |a: f32| (a * 2.0).round() / 2.0;
    (best.0.map(half), half(best.1))
}

/// The pose nearest `(closed, spread)`, within `reach` times `CLEAR_CURL`
/// and `CLEAR_TURN` (no turn at all when `turn` is false), that leaves the
/// finger least far inside the surface.
fn clear(
    chain: &Chain,
    contact: &Contact,
    (closed, spread): ([f32; 3], f32),
    (reach, turn): (f32, bool),
    joint: &impl Fn(usize, f32, f32) -> Quaternion,
) -> ([f32; 3], f32) {
    let first = usize::from(chain.thumb);
    let turn_reach = if turn { CLEAR_TURN * reach } else { 0.0 };
    let curl_reach = CLEAR_CURL * reach;
    let cost = |flex: [f32; 3], turn: f32| {
        let d = depths(chain, contact, flex, turn, joint);
        let inside: f32 = d.iter().map(|x| (x - TOLERANCE).max(0.0).powi(2)).sum();
        let moved: f32 = (first..3)
            .map(|j| (flex[j] - closed[j]).powi(2))
            .sum::<f32>()
            + (turn - spread).powi(2);
        INSIDE * inside + MOVED * moved
    };
    let mut best = (closed, spread);
    let mut best_cost = cost(closed, spread);
    if best_cost <= MOVED {
        return best;
    }
    // Search the whole window, then refine about the best.
    let step = 3.0 * reach.max(1.0);
    let n = (curl_reach / step).round() as i32;
    let m = (turn_reach / step).round() as i32;
    for s in -m..=m {
        for a in -n..=n {
            for b in -n..=n {
                for c in -n..=n {
                    let flex = [
                        closed[0] + a as f32 * step,
                        closed[1] + b as f32 * step,
                        closed[2] + c as f32 * step,
                    ];
                    let turn = spread + s as f32 * step;
                    let here = cost(flex, turn);
                    if here < best_cost {
                        (best, best_cost) = ((flex, turn), here);
                    }
                }
            }
        }
    }
    let mut step = step / 2.0;
    while step >= 0.25 {
        loop {
            let mut moved = false;
            for j in 0..4 {
                for sign in [-1.0, 1.0] {
                    let (mut flex, mut turn) = best;
                    if j < 3 {
                        flex[j] = (flex[j] + sign * step)
                            .clamp(closed[j] - curl_reach, closed[j] + curl_reach);
                    } else {
                        turn = (turn + sign * step).clamp(spread - turn_reach, spread + turn_reach);
                    }
                    let here = cost(flex, turn);
                    if here < best_cost {
                        (best, best_cost, moved) = ((flex, turn), here, true);
                    }
                }
            }
            if !moved {
                break;
            }
        }
        step /= 2.0;
    }
    best
}

/// Closes a finger from nearly straight toward fully closed along `shape`,
/// all joints together, stopping each where the finger beyond it would go
/// into the surface. Points already inside with the finger open (a knuckle
/// pressed to the grip) only stop it if closing pushes them further in.
/// `None` if it closed all the way without meeting anything.
fn close_finger(
    chain: &Chain,
    contact: &Contact,
    shape: [f32; 3],
    spread: f32,
    joint: &impl Fn(usize, f32, f32) -> Quaternion,
) -> Option<[f32; 3]> {
    let first = usize::from(chain.thumb);
    let open = shape.map(|a| a * OPEN);
    let extra = if chain.thumb {
        THUMB_CLOSE_EXTRA
    } else {
        CLOSE_EXTRA
    };
    let closed: [f32; 3] =
        std::array::from_fn(|j| (shape[j] * CLOSE + extra[j]).min(CLOSE_MOST[j]));
    let start = depths(chain, contact, open, spread, joint);
    let first_sample = |segment: usize| SAMPLES[..segment].iter().map(|s| s.len()).sum::<usize>();
    let collides = |flex: [f32; 3], from: usize| {
        let now = depths(chain, contact, flex, spread, joint);
        (first_sample(from.max(first))..SAMPLE_COUNT).any(|i| now[i] > start[i].max(0.0) + 0.0005)
    };
    let mut flex = open;
    let mut frozen = [false; 3];
    let mut touched = false;
    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        for j in 0..3 {
            if frozen[j] {
                continue;
            }
            let goal = open[j] + (closed[j] - open[j]) * t;
            let mut candidate = flex;
            candidate[j] = goal;
            if collides(candidate, j) {
                let (mut clear, mut hit) = (flex[j], goal);
                for _ in 0..REFINE {
                    candidate[j] = (clear + hit) * 0.5;
                    if collides(candidate, j) {
                        hit = candidate[j];
                    } else {
                        clear = candidate[j];
                    }
                }
                flex[j] = clear;
                frozen[j] = true;
                touched = true;
            } else {
                flex = candidate;
            }
        }
        if frozen.iter().all(|f| *f) {
            break;
        }
    }
    touched.then_some(flex)
}

/// Palm points (hand-bone units) and the palm's thickness there: a grid over
/// the palm's pads, from the heel of the hand to the knuckles.
const PALM_POINTS: [Vector3; 8] = [
    Vector3::new(-0.032, 0.04, 0.007),
    Vector3::new(-0.011, 0.04, 0.007),
    Vector3::new(0.011, 0.04, 0.007),
    Vector3::new(0.03, 0.04, 0.007),
    Vector3::new(-0.032, 0.08, 0.007),
    Vector3::new(-0.011, 0.08, 0.007),
    Vector3::new(0.011, 0.08, 0.007),
    Vector3::new(0.03, 0.08, 0.007),
];
const PALM_RADIUS: f32 = 0.012;

/// A finger's knuckle in its hand's space, which no curl of the finger
/// moves (the thumb's: its middle joint, moved only by its base, posed at
/// `base`), and the finger's radius there.
pub fn knuckle(chain: &Chain, base: Quaternion, left: bool) -> (Vector3, f32) {
    if chain.thumb {
        let side = if left { -1.0 } else { 1.0 };
        let frame = Transform3D::new(
            Basis::from_quaternion(chain.rests[0] * base),
            chain.offsets[0],
        );
        let offset = Vector3::new(THUMB[1].offset.x * side, 0.0, THUMB[1].offset.z);
        (frame * (chain.offsets[1] + offset), THUMB[1].radius)
    } else {
        (chain.offsets[0] + FINGER[0].offset, FINGER[0].radius)
    }
}

/// How far (field space, m) to move a hand at `hand_to_field` so its palm
/// and `knuckles` (hand-space points and radii, see [`knuckle`]) rest on the
/// surface instead of in it, as an animator slides a hand onto a grip: the
/// least move that clears them, at most `MOST_SEAT` each way.
pub fn seat(field: &Field, hand_to_field: Transform3D, knuckles: &[(Vector3, f32)]) -> Vector3 {
    let scale = hand_to_field.basis.col_a().length();
    let points: Vec<(Vector3, f32)> = PALM_POINTS
        .iter()
        .map(|p| (hand_to_field * *p, PALM_RADIUS))
        .chain(knuckles.iter().map(|(p, r)| (hand_to_field * *p, *r)))
        .collect();
    let cost = |t: Vector3| {
        let inside: f32 = points
            .iter()
            .map(|(p, r)| {
                (r * PRESS * scale - field.distance(*p + t) - TOLERANCE)
                    .max(0.0)
                    .powi(2)
            })
            .sum();
        INSIDE * inside + SEAT * t.length_squared()
    };
    let steps = (MOST_SEAT / SEAT_STEP).round() as i32;
    let mut best = (cost(Vector3::ZERO), Vector3::ZERO);
    for x in -steps..=steps {
        for y in -steps..=steps {
            for z in -steps..=steps {
                let t = Vector3::new(x as f32, y as f32, z as f32) * SEAT_STEP;
                let here = cost(t);
                if here < best.0 {
                    best = (here, t);
                }
            }
        }
    }
    (best.1 * 10000.0).round() / 10000.0
}
/// A hand moves at most this far (m, each way) to rest on the surface, in
/// steps of `SEAT_STEP`; each m² it moves costs `SEAT`, against `INSIDE` per
/// m² of palm or knuckle inside the surface.
const MOST_SEAT: f32 = 0.012;
const SEAT_STEP: f32 = 0.001;
const SEAT: f32 = 2.0e4;

/// How deep the palm goes into the surface (m; negative: clear of it).
pub fn palm_depth(contact: &Contact) -> f32 {
    let scale = contact.scale();
    PALM_POINTS
        .iter()
        .map(|local| {
            PALM_RADIUS * PRESS * scale - contact.field.distance(contact.hand_to_field * *local)
        })
        .fold(f32::MIN, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box from `lo` to `hi` as twelve triangles.
    fn cuboid(lo: Vector3, hi: Vector3) -> Vec<Tri> {
        let c = |x: bool, y: bool, z: bool| {
            Vector3::new(
                if x { hi.x } else { lo.x },
                if y { hi.y } else { lo.y },
                if z { hi.z } else { lo.z },
            )
        };
        let quad = |a: Vector3, b: Vector3, cc: Vector3, d: Vector3| [[a, b, cc], [a, cc, d]];
        let (f, t) = (false, true);
        [
            quad(c(f, f, f), c(t, f, f), c(t, t, f), c(f, t, f)),
            quad(c(f, f, t), c(t, f, t), c(t, t, t), c(f, t, t)),
            quad(c(f, f, f), c(f, t, f), c(f, t, t), c(f, f, t)),
            quad(c(t, f, f), c(t, t, f), c(t, t, t), c(t, f, t)),
            quad(c(f, f, f), c(t, f, f), c(t, f, t), c(f, f, t)),
            quad(c(f, t, f), c(t, t, f), c(t, t, t), c(f, t, t)),
        ]
        .into_iter()
        .flatten()
        .map(|[a, b, cc]| {
            // Every face wound with its front outward, as a closed mesh is.
            let centre = (lo + hi) * 0.5;
            let out = (b - a).cross(cc - a).dot((a + b + cc) / 3.0 - centre) > 0.0;
            if out { [a, b, cc] } else { [a, cc, b] }
        })
        .collect::<Vec<_>>()
        .pipe_closed()
    }

    trait PipeClosed {
        fn pipe_closed(self) -> Vec<Tri>;
    }
    impl PipeClosed for Vec<[Vector3; 3]> {
        fn pipe_closed(self) -> Vec<Tri> {
            assert!(is_closed(&self));
            Tri::closed(&self)
        }
    }

    #[test]
    fn the_field_measures_distance_to_the_nearest_face() {
        let tris = cuboid(
            Vector3::new(-0.02, -0.02, -0.1),
            Vector3::new(0.02, 0.02, 0.1),
        );
        let field = Field::from_triangles(&tris, Vector3::ZERO, Vector3::splat(0.06)).unwrap();
        // 6 mm above the top face, 10 mm off a side, and out of the band.
        assert!((field.distance(Vector3::new(0.0, 0.026, 0.0)) - 0.006).abs() < 1e-3);
        assert!((field.distance(Vector3::new(0.03, 0.0, 0.01)) - 0.01).abs() < 1e-3);
        assert_eq!(field.distance(Vector3::new(0.0, 0.05, 0.0)), BAND);
        assert_eq!(field.distance(Vector3::new(1.0, 0.0, 0.0)), BAND);
    }

    #[test]
    fn nothing_near_the_grip_gives_no_field() {
        let tris = cuboid(Vector3::splat(1.0), Vector3::splat(1.1));
        assert!(Field::from_triangles(&tris, Vector3::ZERO, Vector3::splat(0.05)).is_none());
    }

    /// A straight finger along +Y from the hand's origin, its joints curling
    /// about +X (toward +Z, the palm side).
    fn finger() -> Chain {
        Chain {
            offsets: [
                Vector3::ZERO,
                Vector3::new(0.0, 0.042, 0.0),
                Vector3::new(0.0, 0.034, 0.0),
            ],
            rests: [Quaternion::IDENTITY; 3],
            tip: 0.03,
            thumb: false,
        }
    }

    fn curl(_: usize, flex: f32, _: f32) -> Quaternion {
        Quaternion::from_axis_angle(Vector3::RIGHT, flex.to_radians())
    }

    fn contact(field: &Field) -> Contact<'_> {
        Contact {
            field,
            hand_to_field: Transform3D::IDENTITY,
            left: false,
        }
    }

    #[test]
    fn a_finger_closes_onto_a_bar_instead_of_through_it() {
        // A bar across the finger's path, 25 mm in front of the palm, 40 to
        // 60 mm up the finger. The grip's shape would curl the finger
        // through it.
        let tris = cuboid(
            Vector3::new(-0.05, 0.04, 0.025),
            Vector3::new(0.05, 0.06, 0.045),
        );
        let field =
            Field::from_triangles(&tris, Vector3::new(0.0, 0.05, 0.03), Vector3::splat(0.08))
                .unwrap();
        let contact = contact(&field);
        let shape = [30.0, 40.0, 20.0];
        assert!(
            finger_depth(&finger(), &contact, shape, 0.0, curl) > 0.005,
            "the shape goes in"
        );
        let (fit, _) = fit_finger(&finger(), &contact, (shape, 0.0), (true, false, true), curl);
        let depth = finger_depth(&finger(), &contact, fit, 0.0, curl);
        assert!(depth < 0.001, "fitted {fit:?} is {depth} m in");
        // And it rests on the bar rather than standing off it.
        assert!(depth > -0.004, "fitted {fit:?} stands {depth} m off");
    }

    #[test]
    fn a_finger_laid_into_a_plate_is_nudged_out_of_it() {
        // The finger straight, its tip 3 mm into a plate above it.
        let tris = cuboid(
            Vector3::new(-0.05, 0.095, -0.05),
            Vector3::new(0.05, 0.12, 0.05),
        );
        let field = Field::from_triangles(&tris, Vector3::new(0.0, 0.1, 0.0), Vector3::splat(0.06))
            .unwrap();
        let contact = contact(&field);
        let shape = [0.0, 0.0, 0.0];
        assert!(finger_depth(&finger(), &contact, shape, 0.0, curl) > 0.001);
        let (fit, _) = fit_finger(
            &finger(),
            &contact,
            (shape, 0.0),
            (false, false, true),
            curl,
        );
        assert!(
            finger_depth(&finger(), &contact, fit, 0.0, curl) <= CLEAR_ENOUGH,
            "{fit:?}"
        );
        assert!(fit.iter().all(|a| a.abs() <= CLEAR_CURL), "{fit:?}");
    }

    #[test]
    fn a_finger_with_nothing_in_reach_keeps_its_shape() {
        let tris = cuboid(Vector3::splat(1.0), Vector3::splat(1.1));
        let field =
            Field::from_triangles(&tris, Vector3::splat(1.05), Vector3::splat(0.1)).unwrap();
        let shape = [45.0, 65.0, 35.0];
        assert_eq!(
            fit_finger(
                &finger(),
                &contact(&field),
                (shape, 3.0),
                (true, true, true),
                curl
            ),
            (shape, 3.0)
        );
    }

    #[test]
    fn a_point_deep_inside_a_closed_part_reads_as_inside() {
        // A receiver 30 mm thick: its middle is 15 mm from every face,
        // further than a finger's radius, but inside it.
        let tris = cuboid(
            Vector3::new(-0.015, -0.05, -0.1),
            Vector3::new(0.015, 0.05, 0.1),
        );
        let field = Field::from_triangles(&tris, Vector3::ZERO, Vector3::splat(0.06)).unwrap();
        assert!(
            field.distance(Vector3::ZERO) < -0.012,
            "{}",
            field.distance(Vector3::ZERO)
        );
        assert!(field.distance(Vector3::new(0.025, 0.0, 0.0)) > 0.008);
        // Further in than the band measures, still inside.
        let thick = cuboid(Vector3::splat(-0.05), Vector3::splat(0.05));
        let field = Field::from_triangles(&thick, Vector3::ZERO, Vector3::splat(0.06)).unwrap();
        assert!(field.distance(Vector3::ZERO) < 0.0);
        // An open sheet has no inside.
        let sheet = vec![Tri::open([
            Vector3::new(-0.1, 0.0, -0.1),
            Vector3::new(0.1, 0.0, -0.1),
            Vector3::new(0.0, 0.0, 0.1),
        ])];
        let field = Field::from_triangles(&sheet, Vector3::ZERO, Vector3::splat(0.03)).unwrap();
        assert!(field.distance(Vector3::new(0.0, -0.01, 0.0)) > 0.009);
    }

    #[test]
    fn points_off_an_edge_or_a_corner_read_as_outside() {
        // Just past a box's edge and its corner, where the nearest point is
        // shared by two or three faces, any of which may be nearest.
        let tris = cuboid(Vector3::splat(-0.02), Vector3::splat(0.02));
        let field = Field::from_triangles(&tris, Vector3::ZERO, Vector3::splat(0.04)).unwrap();
        for p in [Vector3::new(0.024, 0.024, 0.0), Vector3::splat(0.024)] {
            assert!(field.distance(p) > 0.0, "{p:?}: {}", field.distance(p));
        }
        for p in [Vector3::new(0.016, 0.016, 0.0), Vector3::splat(0.016)] {
            assert!(field.distance(p) < 0.0, "{p:?}: {}", field.distance(p));
        }
    }

    #[test]
    fn a_finger_buried_in_a_closed_part_is_inside_it() {
        let tris = cuboid(
            Vector3::new(-0.02, -0.05, -0.05),
            Vector3::new(0.02, 0.15, 0.05),
        );
        let field =
            Field::from_triangles(&tris, Vector3::new(0.0, 0.05, 0.0), Vector3::splat(0.08))
                .unwrap();
        let shape = [0.0, 0.0, 0.0];
        assert!(finger_depth(&finger(), &contact(&field), shape, 0.0, curl) > 0.01);
    }

    #[test]
    fn a_palm_in_a_plate_reads_as_inside_it() {
        let tris = cuboid(
            Vector3::new(-0.1, 0.0, 0.015),
            Vector3::new(0.1, 0.12, 0.03),
        );
        let field =
            Field::from_triangles(&tris, Vector3::new(0.0, 0.06, 0.02), Vector3::splat(0.08))
                .unwrap();
        assert!(palm_depth(&contact(&field)) > 0.002);
        let back = Contact {
            hand_to_field: Transform3D::new(Basis::IDENTITY, Vector3::new(0.0, 0.0, -0.01)),
            ..contact(&field)
        };
        assert!(palm_depth(&back) < 0.0);
    }
}
