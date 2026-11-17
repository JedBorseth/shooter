//! Team Deathmatch: teams, the match clock and score, and where players spawn.
//!
//! Two teams race to a kill limit before the clock runs out. Other modes
//! ([`Mode`]) reuse the clock and the phases; Gun Game is every player for
//! themselves (`crate::gun_game`). A match goes through these phases:
//!
//! - [`Phase::Lobby`]: players join, pick a loadout, and can run around and
//!   shoot, but nothing counts. It ends when every human player is ready.
//! - [`Phase::Countdown`]: a few seconds to get ready. Everyone respawns at
//!   their team's side when it ends.
//! - [`Phase::Playing`]: kills score for the team.
//! - [`Phase::Ended`]: the result screen, then the next match counts down.
//!
//! The server runs this; clients only show what it sends
//! (`net::protocol::GameEvent::Match`).

use glam::Vec3;

use crate::{gun_game, rng};

/// A team index: 0 or 1.
pub type Team = u8;
pub const TEAM_COUNT: usize = 2;

/// The game a server runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Two teams race to a kill limit.
    #[default]
    TeamDeathmatch,
    /// Every player for themselves, up a ladder of weapons.
    GunGame(gun_game::Rules),
}

impl Mode {
    /// Parses a mode name as servers take it on the command line ("tdm" or
    /// "gungame"), with default rules.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "" | "tdm" | "teamdeathmatch" | "team_deathmatch" => Some(Self::TeamDeathmatch),
            "gungame" | "gun_game" | "gun-game" => Some(Self::GunGame(gun_game::Rules::default())),
            _ => None,
        }
    }

    /// The short name [`Mode::parse`] takes.
    pub fn key(&self) -> &'static str {
        match self {
            Self::TeamDeathmatch => "tdm",
            Self::GunGame(_) => "gungame",
        }
    }

    /// The name players see, as in server browsers.
    pub fn label(&self) -> &'static str {
        match self {
            Self::TeamDeathmatch => "Team Deathmatch",
            Self::GunGame(_) => "Gun Game",
        }
    }

    /// Whether every other player is an enemy. Teams then mean nothing:
    /// everyone is on team 0.
    pub fn free_for_all(&self) -> bool {
        matches!(self, Self::GunGame(_))
    }
}

/// Tunable match rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchRules {
    /// Kills a team needs to win.
    pub score_limit: u32,
    /// Seconds of play before the team ahead wins (a draw if level).
    pub time_limit: f32,
    /// Seconds between everyone being ready and the start.
    pub countdown: f32,
    /// Seconds the result screen stays up before the next countdown.
    pub results: f32,
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            score_limit: 75,
            time_limit: 10.0 * 60.0,
            countdown: 5.0,
            results: 15.0,
        }
    }
}

/// Where a match is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Lobby,
    Countdown,
    Playing,
    /// `winner` is a team in Team Deathmatch and a player id in free-for-all
    /// modes; `None` is a draw.
    Ended {
        winner: Option<u8>,
    },
}

/// What changed in one [`MatchState::tick`], so the server can respawn
/// everyone or reset the scores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    /// The countdown ended: respawn everyone at their side and reset scores.
    Started,
    /// The clock or the score limit ended the match (`winner` as in
    /// [`Phase::Ended`]).
    Ended { winner: Option<u8> },
    /// The results are over; the next match counts down.
    NextCountdown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchState {
    pub phase: Phase,
    /// Seconds left in the current phase (0 in the lobby, which has no clock).
    pub time_left: f32,
    pub scores: [u32; TEAM_COUNT],
}

impl Default for MatchState {
    fn default() -> Self {
        Self {
            phase: Phase::Lobby,
            time_left: 0.0,
            scores: [0; TEAM_COUNT],
        }
    }
}

impl MatchState {
    /// Kills count toward the score only while playing.
    pub fn scoring(&self) -> bool {
        self.phase == Phase::Playing
    }

    /// Advances the clock. `all_ready` is whether every human player is ready
    /// (and there is at least one); it only matters in the lobby. `leader`
    /// wins if the clock runs out: [`MatchState::team_leader`] in Team
    /// Deathmatch, the player furthest ahead in other modes.
    pub fn tick(
        &mut self,
        rules: &MatchRules,
        dt: f32,
        all_ready: bool,
        leader: Option<u8>,
    ) -> Option<Transition> {
        match self.phase {
            Phase::Lobby => {
                if all_ready {
                    self.phase = Phase::Countdown;
                    self.time_left = rules.countdown;
                }
                None
            }
            Phase::Countdown => {
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                self.phase = Phase::Playing;
                self.time_left = rules.time_limit;
                self.scores = [0; TEAM_COUNT];
                Some(Transition::Started)
            }
            Phase::Playing => {
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                Some(self.finish(rules, leader))
            }
            Phase::Ended { .. } => {
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                self.phase = Phase::Countdown;
                self.time_left = rules.countdown;
                Some(Transition::NextCountdown)
            }
        }
    }

    /// Counts a kill of `victim_team` by `killer_team`. Suicides, falls, and
    /// teamkills score nothing. Returns the end of the match if this kill
    /// reached the score limit.
    pub fn on_kill(
        &mut self,
        rules: &MatchRules,
        killer_team: Option<Team>,
        victim_team: Team,
    ) -> Option<Transition> {
        if !self.scoring() {
            return None;
        }
        let killer = killer_team.filter(|t| *t != victim_team && usize::from(*t) < TEAM_COUNT)?;
        let score = &mut self.scores[usize::from(killer)];
        *score += 1;
        (*score >= rules.score_limit).then(|| self.finish(rules, Some(killer)))
    }

    /// Returns to the lobby, for when every human has left.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The team with more kills, or `None` when level.
    pub fn team_leader(&self) -> Option<Team> {
        match self.scores[0].cmp(&self.scores[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// Ends the match now with `winner` (as in [`Phase::Ended`]), for modes
    /// whose win the server decides.
    pub fn finish(&mut self, rules: &MatchRules, winner: Option<u8>) -> Transition {
        self.phase = Phase::Ended { winner };
        self.time_left = rules.results;
        Transition::Ended { winner }
    }
}

/// The team a new player joins: the smaller one, or `tie` when level. Humans
/// break ties toward team 0 and bots toward team 1, so a server's bots start
/// across from its first human.
pub fn assign_team(counts: [usize; TEAM_COUNT], tie: Team) -> Team {
    match counts[0].cmp(&counts[1]) {
        std::cmp::Ordering::Less => 0,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => tie,
    }
}

/// A place a player can spawn, and the team whose side of the map it is on
/// (`None` for spawns in the middle that either team may use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPoint {
    pub position: Vec3,
    /// Direction the player faces, in radians (yaw, as in `PlayerInput`).
    pub yaw: f32,
    pub team: Option<Team>,
}

/// Someone a spawn should keep away from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub position: Vec3,
    /// An enemy (true), or a teammate (false), whose presence makes a spawn safer.
    pub enemy: bool,
}

/// How [`choose_spawn`] weighs a spawn point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRules {
    /// Keep enemies outside this distance whenever a safer point exists.
    pub min_enemy_distance: f32,
    /// Keep living players outside this radius to avoid overlapping bodies.
    pub occupied_radius: f32,
    /// Enemies closer than this make a spawn worse, linearly.
    pub enemy_radius: f32,
    /// Penalty for each enemy that can see the spawn.
    pub seen_penalty: f32,
    /// Deaths in the last few seconds within this radius make a spawn worse.
    pub death_radius: f32,
    pub death_penalty: f32,
    /// Bonus for each teammate within `enemy_radius`.
    pub teammate_bonus: f32,
    /// Bonus for a spawn on the player's own side.
    pub side_bonus: f32,
    /// Penalty for a spawn on the enemy's side.
    pub enemy_side_penalty: f32,
}

impl Default for SpawnRules {
    fn default() -> Self {
        Self {
            min_enemy_distance: 8.0,
            occupied_radius: 1.5,
            enemy_radius: 25.0,
            seen_penalty: 60.0,
            death_radius: 8.0,
            death_penalty: 25.0,
            teammate_bonus: 4.0,
            side_bonus: 10.0,
            enemy_side_penalty: 30.0,
        }
    }
}

/// Most random score [`choose_spawn`] adds to break ties.
const SPAWN_NOISE: f32 = 2.0;

/// Picks the safest spawn point for a player of `team`. `seen_by_enemies(i)`
/// counts the enemies with a line of sight to spawn `i` (the server raycasts
/// it). `recent_deaths` are where players died in the last few seconds.
/// `seed` breaks ties between equally good spawns, so players don't all
/// stack on one point.
///
/// Clear points always beat watched points, points near enemies, and occupied
/// points (in that order). If every point is dangerous, choose the least bad
/// available tier; team preference never prevents a safe spawn flip.
/// Returns `None` only when there are no spawn points.
pub fn choose_spawn(
    points: &[SpawnPoint],
    team: Team,
    threats: &[Threat],
    recent_deaths: &[Vec3],
    rules: &SpawnRules,
    mut seen_by_enemies: impl FnMut(usize) -> u32,
    seed: u32,
) -> Option<usize> {
    // Safety tiers beat soft bonuses: clear, watched, enemy nearby, occupied.
    let mut best: Option<(usize, u8, f32)> = None;
    for (i, point) in points.iter().enumerate() {
        let mut score = 0.0;
        let mut danger = 0;
        score += match point.team {
            Some(t) if t == team => rules.side_bonus,
            Some(_) => -rules.enemy_side_penalty,
            None => 0.0,
        };
        for threat in threats {
            let distance = threat.position.distance(point.position);
            if distance < rules.occupied_radius {
                danger = 3;
            } else if threat.enemy && distance < rules.min_enemy_distance {
                danger = danger.max(2);
            }
            if distance >= rules.enemy_radius {
                continue;
            }
            if threat.enemy {
                // Up to the full radius' worth of penalty for an enemy on top of it.
                score -= rules.enemy_radius - distance;
            } else {
                score += rules.teammate_bonus;
            }
        }
        for death in recent_deaths {
            if death.distance(point.position) < rules.death_radius {
                score -= rules.death_penalty;
            }
        }
        // Every candidate needs a visibility tier before comparing soft scores.
        let seen = seen_by_enemies(i);
        if seen > 0 {
            danger = danger.max(1);
        }
        score -= rules.seen_penalty * seen as f32;
        score += rng::unit(seed, i as u32) * SPAWN_NOISE;
        if best.is_none_or(|(_, d, b)| danger < d || (danger == d && score > b)) {
            best = Some((i, danger, score));
        }
    }
    best.map(|(i, _, _)| i)
}

/// The spawn points for the start of a match: the team's own side, spread out
/// so teammates don't spawn inside each other. `slot` is the player's place in
/// their team.
pub fn start_spawn(points: &[SpawnPoint], team: Team, slot: usize) -> Option<usize> {
    let own: Vec<usize> = (0..points.len())
        .filter(|i| points[*i].team == Some(team))
        .collect();
    if own.is_empty() {
        return (!points.is_empty()).then(|| slot % points.len());
    }
    Some(own[slot % own.len()])
}

/// A player name as the server keeps it: printable, trimmed, at most
/// [`MAX_NAME_CHARS`] characters, and never empty.
pub fn clean_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect::<String>()
        .trim()
        .to_owned();
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

pub const MAX_NAME_CHARS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> MatchRules {
        MatchRules {
            score_limit: 3,
            time_limit: 10.0,
            countdown: 2.0,
            results: 4.0,
        }
    }

    fn run(state: &mut MatchState, seconds: f32, all_ready: bool) -> Vec<Transition> {
        let mut out = Vec::new();
        for _ in 0..(seconds * 10.0).round() as u32 {
            let leader = state.team_leader();
            out.extend(state.tick(&rules(), 0.1, all_ready, leader));
        }
        out
    }

    #[test]
    fn the_lobby_waits_for_everyone_to_be_ready() {
        let mut state = MatchState::default();
        assert!(run(&mut state, 30.0, false).is_empty());
        assert_eq!(state.phase, Phase::Lobby);
        state.tick(&rules(), 0.1, true, None);
        assert_eq!(state.phase, Phase::Countdown);
        assert_eq!(run(&mut state, 2.05, false), vec![Transition::Started]);
        assert_eq!(state.phase, Phase::Playing);
    }

    #[test]
    fn kills_only_count_while_playing() {
        let mut state = MatchState::default();
        assert_eq!(state.on_kill(&rules(), Some(0), 1), None);
        assert_eq!(state.scores, [0, 0]);
        state.phase = Phase::Playing;
        state.time_left = 10.0;
        state.on_kill(&rules(), Some(0), 1);
        // Teamkills, suicides, and falls score nothing.
        state.on_kill(&rules(), Some(1), 1);
        state.on_kill(&rules(), None, 1);
        assert_eq!(state.scores, [1, 0]);
    }

    #[test]
    fn the_score_limit_ends_the_match() {
        let mut state = MatchState {
            phase: Phase::Playing,
            time_left: 10.0,
            scores: [0, 2],
        };
        assert_eq!(
            state.on_kill(&rules(), Some(1), 0),
            Some(Transition::Ended { winner: Some(1) })
        );
        assert_eq!(state.phase, Phase::Ended { winner: Some(1) });
        // No more scoring on the results screen.
        state.on_kill(&rules(), Some(0), 1);
        assert_eq!(state.scores, [0, 3]);
    }

    #[test]
    fn the_clock_ends_the_match_and_the_next_one_counts_down() {
        let mut state = MatchState {
            phase: Phase::Playing,
            time_left: 1.0,
            scores: [4, 4],
        };
        assert_eq!(
            run(&mut state, 1.05, false),
            vec![Transition::Ended { winner: None }]
        );
        assert_eq!(
            run(&mut state, 4.05, false),
            vec![Transition::NextCountdown]
        );
        assert_eq!(state.phase, Phase::Countdown);
        // Scores stay up through the countdown and reset at the start.
        assert_eq!(state.scores, [4, 4]);
        run(&mut state, 2.05, false);
        assert_eq!(state.scores, [0, 0]);
    }

    #[test]
    fn the_server_can_end_a_match_for_a_player() {
        let mut state = MatchState {
            phase: Phase::Playing,
            time_left: 10.0,
            scores: [0, 0],
        };
        assert_eq!(
            state.finish(&rules(), Some(42)),
            Transition::Ended { winner: Some(42) }
        );
        assert_eq!(state.phase, Phase::Ended { winner: Some(42) });
        assert_eq!(state.time_left, rules().results);
        // When the clock runs out, the leader the server names wins.
        let mut state = MatchState {
            phase: Phase::Playing,
            time_left: 0.05,
            scores: [0, 0],
        };
        assert_eq!(
            state.tick(&rules(), 0.1, false, Some(7)),
            Some(Transition::Ended { winner: Some(7) })
        );
    }

    #[test]
    fn modes_parse_from_their_keys() {
        assert_eq!(Mode::parse(""), Some(Mode::TeamDeathmatch));
        assert_eq!(Mode::parse("tdm"), Some(Mode::TeamDeathmatch));
        let gun_game = Mode::parse("GunGame").unwrap();
        assert!(gun_game.free_for_all());
        assert_eq!(Mode::parse(gun_game.key()), Some(gun_game));
        assert_eq!(gun_game.label(), "Gun Game");
        assert!(!Mode::TeamDeathmatch.free_for_all());
        assert_eq!(Mode::parse("domination"), None);
    }

    #[test]
    fn new_players_balance_the_teams() {
        assert_eq!(assign_team([0, 0], 0), 0);
        assert_eq!(assign_team([0, 0], 1), 1);
        assert_eq!(assign_team([1, 0], 0), 1);
        assert_eq!(assign_team([3, 4], 1), 0);
    }

    fn point(x: f32, team: Option<Team>) -> SpawnPoint {
        SpawnPoint {
            position: Vec3::new(x, 0.0, 0.0),
            yaw: 0.0,
            team,
        }
    }

    #[test]
    fn spawns_avoid_enemies_and_their_sightlines() {
        let points = [point(0.0, None), point(50.0, None), point(100.0, None)];
        let threats = [Threat {
            position: Vec3::new(5.0, 0.0, 0.0),
            enemy: true,
        }];
        let rules = SpawnRules::default();
        let pick = |seen: &dyn Fn(usize) -> u32| {
            choose_spawn(&points, 0, &threats, &[], &rules, seen, 1).unwrap()
        };
        // Far from the enemy, and the far one is seen: the middle one wins.
        assert_eq!(pick(&|i| u32::from(i == 2)), 1);
        // Nobody sees anything: either far spawn, never the one next to the enemy.
        assert_ne!(pick(&|_| 0), 0);
    }

    #[test]
    fn spawns_prefer_the_own_side_and_avoid_recent_deaths() {
        let points = [
            point(0.0, Some(0)),
            point(60.0, None),
            point(120.0, Some(1)),
        ];
        let rules = SpawnRules::default();
        let choose = |team, deaths: &[Vec3]| {
            choose_spawn(&points, team, &[], deaths, &rules, |_| 0, 7).unwrap()
        };
        assert_eq!(choose(0, &[]), 0);
        assert_eq!(choose(1, &[]), 2);
        assert_eq!(choose(0, &[Vec3::ZERO, Vec3::ZERO]), 1);
    }

    #[test]
    fn safety_beats_team_bias_and_teammate_bonuses() {
        let points = [point(0.0, Some(0)), point(60.0, Some(1))];
        let friends = vec![
            Threat {
                position: Vec3::X * 10.0,
                enemy: false
            };
            20
        ];
        for seed in 0..100 {
            assert_eq!(
                choose_spawn(
                    &points,
                    0,
                    &friends,
                    &[],
                    &SpawnRules::default(),
                    |i| u32::from(i == 0),
                    seed
                ),
                Some(1)
            );
        }
    }

    #[test]
    fn enemies_force_a_flip_even_when_the_home_spawn_is_hidden() {
        let points = [point(0.0, Some(0)), point(60.0, Some(1))];
        let enemies = [Threat {
            position: Vec3::X * 4.0,
            enemy: true,
        }];
        assert_eq!(
            choose_spawn(&points, 0, &enemies, &[], &SpawnRules::default(), |_| 0, 1),
            Some(1)
        );
    }

    #[test]
    fn occupied_spawns_lose_even_when_the_other_point_is_watched() {
        let points = [point(0.0, Some(0)), point(60.0, Some(1))];
        for enemy in [false, true] {
            let players = [Threat {
                position: Vec3::ZERO,
                enemy,
            }];
            assert_eq!(
                choose_spawn(&points, 0, &players, &[], &SpawnRules::default(), |_| 1, 1),
                Some(1)
            );
        }
    }

    #[test]
    fn dangerous_maps_still_return_a_spawn_and_empty_maps_return_none() {
        let points = [point(0.0, None), point(60.0, None)];
        let choose = |points: &[SpawnPoint]| {
            choose_spawn(points, 0, &[], &[], &SpawnRules::default(), |_| 1, 1)
        };
        assert!(choose(&points).is_some());
        assert_eq!(choose(&[]), None);
    }

    #[test]
    fn start_spawns_spread_a_team_over_its_side() {
        let points = [
            point(0.0, Some(0)),
            point(1.0, Some(1)),
            point(2.0, Some(0)),
        ];
        assert_eq!(start_spawn(&points, 0, 0), Some(0));
        assert_eq!(start_spawn(&points, 0, 1), Some(2));
        assert_eq!(start_spawn(&points, 0, 2), Some(0));
        assert_eq!(start_spawn(&points, 1, 5), Some(1));
        assert_eq!(start_spawn(&[], 1, 5), None);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Ace\n ", "Player 1"), "Ace");
        assert_eq!(clean_name("\t", "Player 1"), "Player 1");
        assert_eq!(
            clean_name("abcdefghijklmnopqrstuvwxyz", "x")
                .chars()
                .count(),
            16
        );
    }
}
