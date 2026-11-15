//! Kill scoring: points, streaks, multi-kills, and medals.
//!
//! Every kill pays a flat amount, then bonuses stack on top for how it
//! happened. The HUD shows each award as a "+N" popup, and medals also get a
//! banner. This is the rule book only; the client feeds it confirmed kills
//! now, and the dedicated server will own it once kills are authoritative.

/// Tunable scoring rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreRules {
    /// Points for any kill.
    pub kill: u32,
    /// Bonus when the killing hit was a headshot.
    pub headshot: u32,
    /// Bonus for a kill at or beyond `longshot_distance` metres.
    pub longshot: u32,
    pub longshot_distance: f32,
    /// Bonus for a kill within `point_blank_distance` metres.
    pub point_blank: u32,
    pub point_blank_distance: f32,
    /// Bonus per extra kill when one shot kills more than one target.
    pub collateral: u32,
    /// Seconds after a kill in which the next kill extends a multi-kill.
    pub multikill_window: f32,
    /// Bonus per kill in a chain, starting at the second kill.
    pub multikill: u32,
    /// Every `streak_step` kills without dying awards a streak medal.
    pub streak_step: u32,
    pub streak: u32,
}

impl Default for ScoreRules {
    fn default() -> Self {
        Self {
            kill: 100,
            headshot: 50,
            longshot: 50,
            longshot_distance: 40.0,
            point_blank: 25,
            point_blank_distance: 2.5,
            collateral: 100,
            multikill_window: 4.0,
            multikill: 50,
            streak_step: 5,
            streak: 100,
        }
    }
}

/// One confirmed kill, as seen by the shooter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kill {
    /// Metres from the shooter's eye to the killing hit.
    pub distance: f32,
    pub headshot: bool,
}

/// What an award was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Award {
    Kill,
    Headshot,
    Longshot,
    PointBlank,
    Collateral,
    /// Two, three, or more kills in quick succession. Holds the chain length.
    MultiKill(u32),
    /// Kills without dying, in multiples of the streak step.
    Streak(u32),
}

impl Award {
    /// Whether the HUD gives this its own banner, not just a points line.
    /// Only the rarer awards get one, so banners keep up with the action:
    /// headshots and distance bonuses are frequent and show as lines only.
    pub fn is_medal(self) -> bool {
        matches!(
            self,
            Award::Collateral | Award::MultiKill(_) | Award::Streak(_)
        )
    }

    /// Text for popups and banners.
    pub fn label(self) -> String {
        match self {
            Award::Kill => "Kill".to_owned(),
            Award::Headshot => "Headshot".to_owned(),
            Award::Longshot => "Longshot".to_owned(),
            Award::PointBlank => "Point Blank".to_owned(),
            Award::Collateral => "Collateral".to_owned(),
            Award::MultiKill(2) => "Double Kill".to_owned(),
            Award::MultiKill(3) => "Triple Kill".to_owned(),
            Award::MultiKill(4) => "Quad Kill".to_owned(),
            Award::MultiKill(n) => format!("{n}x Multi Kill"),
            Award::Streak(n) => format!("{n} Kill Streak"),
        }
    }
}

/// An award and the points it paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scored {
    pub award: Award,
    pub points: u32,
}

/// One player's score for a match.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreState {
    pub score: u32,
    pub kills: u32,
    pub deaths: u32,
    /// Kills since the last death.
    pub streak: u32,
    /// Kills in the current multi-kill chain.
    chain: u32,
    /// Seconds left to extend the chain.
    chain_left: f32,
}

impl ScoreState {
    /// Advances timers.
    pub fn tick(&mut self, dt: f32) {
        self.chain_left = (self.chain_left - dt).max(0.0);
        if self.chain_left == 0.0 {
            self.chain = 0;
        }
    }

    /// Scores the kills made by one shot (a shotgun blast or a bullet that
    /// killed several targets counts once, with a collateral bonus).
    /// Returns the awards in display order: kill points first, then medals.
    pub fn on_shot_kills(&mut self, rules: &ScoreRules, kills: &[Kill]) -> Vec<Scored> {
        let mut awards = Vec::new();
        if kills.is_empty() {
            return awards;
        }
        let mut push = |award, points| awards.push(Scored { award, points });

        for kill in kills {
            push(Award::Kill, rules.kill);
            if kill.headshot {
                push(Award::Headshot, rules.headshot);
            }
            if kill.distance >= rules.longshot_distance {
                push(Award::Longshot, rules.longshot);
            } else if kill.distance <= rules.point_blank_distance {
                push(Award::PointBlank, rules.point_blank);
            }
        }
        let extra = kills.len() as u32 - 1;
        if extra > 0 {
            push(Award::Collateral, rules.collateral * extra);
        }

        let before = self.chain;
        self.chain += kills.len() as u32;
        self.chain_left = rules.multikill_window;
        if self.chain >= 2 {
            let new_links = self.chain - before.max(1);
            push(Award::MultiKill(self.chain), rules.multikill * new_links);
        }

        let streak_before = self.streak;
        self.streak += kills.len() as u32;
        if rules.streak_step > 0 {
            let step = rules.streak_step;
            let crossed = self.streak / step - streak_before / step;
            if crossed > 0 {
                let reached = self.streak / step * step;
                push(Award::Streak(reached), rules.streak * crossed);
            }
        }

        self.kills += kills.len() as u32;
        self.score += awards.iter().map(|a| a.points).sum::<u32>();
        awards
    }

    /// Ends the current streak and chain.
    pub fn on_death(&mut self) {
        self.deaths += 1;
        self.streak = 0;
        self.chain = 0;
        self.chain_left = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: Kill = Kill {
        distance: 15.0,
        headshot: false,
    };

    fn awards(scored: &[Scored]) -> Vec<Award> {
        scored.iter().map(|s| s.award).collect()
    }

    #[test]
    fn plain_kill_pays_base_points() {
        let mut s = ScoreState::default();
        let out = s.on_shot_kills(&ScoreRules::default(), &[BODY]);
        assert_eq!(awards(&out), [Award::Kill]);
        assert_eq!((s.score, s.kills, s.streak), (100, 1, 1));
    }

    #[test]
    fn no_kills_scores_nothing() {
        let mut s = ScoreState::default();
        assert!(s.on_shot_kills(&ScoreRules::default(), &[]).is_empty());
        assert_eq!(s, ScoreState::default());
    }

    #[test]
    fn headshot_and_distance_bonuses_stack() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        let far = Kill {
            distance: 60.0,
            headshot: true,
        };
        let out = s.on_shot_kills(&rules, &[far]);
        assert_eq!(
            awards(&out),
            [Award::Kill, Award::Headshot, Award::Longshot]
        );
        assert_eq!(s.score, 200);

        s.tick(10.0);
        let close = Kill {
            distance: 1.0,
            headshot: false,
        };
        let out = s.on_shot_kills(&rules, &[close]);
        assert_eq!(awards(&out), [Award::Kill, Award::PointBlank]);
    }

    #[test]
    fn kills_inside_the_window_chain_into_multikills() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        s.on_shot_kills(&rules, &[BODY]);
        s.tick(rules.multikill_window * 0.9);
        let out = s.on_shot_kills(&rules, &[BODY]);
        assert_eq!(out.last().unwrap().award, Award::MultiKill(2));
        s.tick(1.0);
        let out = s.on_shot_kills(&rules, &[BODY]);
        assert_eq!(out.last().unwrap().award, Award::MultiKill(3));
        assert_eq!(Award::MultiKill(3).label(), "Triple Kill");
    }

    #[test]
    fn chain_breaks_after_the_window() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        s.on_shot_kills(&rules, &[BODY]);
        s.tick(rules.multikill_window + 0.01);
        let out = s.on_shot_kills(&rules, &[BODY]);
        assert_eq!(awards(&out), [Award::Kill]);
    }

    #[test]
    fn one_shot_two_kills_is_a_collateral_double() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        let out = s.on_shot_kills(&rules, &[BODY, BODY]);
        assert_eq!(
            awards(&out),
            [
                Award::Kill,
                Award::Kill,
                Award::Collateral,
                Award::MultiKill(2)
            ]
        );
        assert_eq!(s.score, 100 + 100 + 100 + 50);
        assert_eq!(s.kills, 2);
    }

    #[test]
    fn streak_medal_every_five_and_reset_on_death() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        let mut medals = Vec::new();
        for _ in 0..10 {
            s.tick(10.0);
            medals.extend(
                s.on_shot_kills(&rules, &[BODY])
                    .into_iter()
                    .filter(|a| matches!(a.award, Award::Streak(_))),
            );
        }
        assert_eq!(awards(&medals), [Award::Streak(5), Award::Streak(10)]);
        s.on_death();
        assert_eq!((s.streak, s.deaths, s.kills), (0, 1, 10));
    }

    #[test]
    fn collateral_crossing_a_streak_step_counts_once() {
        let rules = ScoreRules::default();
        let mut s = ScoreState::default();
        for _ in 0..4 {
            s.tick(10.0);
            s.on_shot_kills(&rules, &[BODY]);
        }
        s.tick(10.0);
        let out = s.on_shot_kills(&rules, &[BODY, BODY]);
        assert!(out.contains(&Scored {
            award: Award::Streak(5),
            points: 100
        }));
    }

    #[test]
    fn only_rare_awards_get_banners() {
        assert!(!Award::Kill.is_medal());
        assert!(!Award::Headshot.is_medal());
        assert!(!Award::Longshot.is_medal());
        assert!(Award::Collateral.is_medal());
        assert!(Award::MultiKill(2).is_medal());
        assert!(Award::Streak(5).is_medal());
    }
}
