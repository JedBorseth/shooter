//! Bullet penetration through thin cover.
//!
//! Each weapon has a penetration budget in metres of wood (`WeaponDef::
//! penetration`). Crossing a wall spends its thickness times the material's
//! resistance, and the round keeps a share of its damage that depends on the
//! material and on how much of the budget the wall used. A round stops when a
//! wall costs more than the budget left, or after [`MAX_WALLS`] walls.
//!
//! The engine measures the walls (see `rust/game/src/penetration.rs`); this
//! module only holds the rules, so the client's local hits and the server's
//! agree.

/// The most walls one round can cross.
pub const MAX_WALLS: usize = 2;

/// Walls thicker than this (metres) are never measured through.
pub const MAX_THICKNESS: f32 = 0.6;

/// What a wall is made of, from its `surface` tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Wood,
    Metal,
    Concrete,
    Glass,
}

impl Material {
    /// The material for a `surface` tag. Untagged geometry is concrete.
    pub fn from_surface(surface: &str) -> Self {
        match surface {
            "wood" => Self::Wood,
            "metal" => Self::Metal,
            "glass" => Self::Glass,
            _ => Self::Concrete,
        }
    }

    /// Budget (metres of wood) one metre of this material costs.
    pub fn resistance(self) -> f32 {
        match self {
            Self::Glass => 0.4,
            Self::Wood => 1.0,
            Self::Concrete => 3.0,
            Self::Metal => 5.0,
        }
    }

    /// Share of the damage a round keeps after crossing even a sliver of it.
    pub fn damage_kept(self) -> f32 {
        match self {
            Self::Glass => 0.95,
            Self::Wood => 0.8,
            Self::Concrete => 0.6,
            Self::Metal => 0.55,
        }
    }
}

/// One round's progress through walls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Penetration {
    /// Budget left, in metres of wood.
    pub budget: f32,
    /// Damage multiplier for anything hit from here on.
    pub damage_scale: f32,
    pub walls: u8,
}

impl Penetration {
    pub fn new(budget: f32) -> Self {
        Self {
            budget: budget.max(0.0),
            damage_scale: 1.0,
            walls: 0,
        }
    }

    /// The thickest wall of `material` this round could still cross (metres).
    pub fn reach(&self, material: Material) -> f32 {
        if usize::from(self.walls) >= MAX_WALLS {
            return 0.0;
        }
        (self.budget / material.resistance()).min(MAX_THICKNESS)
    }

    /// Crosses a wall `thickness` metres thick. False (and unchanged) if the
    /// round stops in it.
    pub fn cross(&mut self, material: Material, thickness: f32) -> bool {
        let cost = thickness.max(0.0) * material.resistance();
        if usize::from(self.walls) >= MAX_WALLS || cost >= self.budget {
            return false;
        }
        // The more of the budget the wall takes, the more damage it takes.
        let used = cost / self.budget;
        self.damage_scale *= material.damage_kept() * (1.0 - 0.5 * used);
        self.budget -= cost;
        self.walls += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rifle_goes_through_a_plank_but_not_a_concrete_wall() {
        let mut round = Penetration::new(0.3);
        assert!(round.cross(Material::Wood, 0.04));
        assert!(round.damage_scale > 0.7 && round.damage_scale < 0.8);
        assert!(!round.cross(Material::Concrete, 0.2));
        assert_eq!(round.walls, 1, "a failed crossing changes nothing");
    }

    #[test]
    fn thicker_and_harder_walls_keep_less_damage() {
        let scale = |material, thickness| {
            let mut round = Penetration::new(0.6);
            round.cross(material, thickness);
            round.damage_scale
        };
        assert!(scale(Material::Wood, 0.05) > scale(Material::Wood, 0.3));
        assert!(scale(Material::Wood, 0.05) > scale(Material::Metal, 0.05));
        assert!(scale(Material::Glass, 0.01) > scale(Material::Wood, 0.01));
    }

    #[test]
    fn rounds_stop_after_the_wall_limit_or_with_no_budget() {
        let mut round = Penetration::new(10.0);
        for _ in 0..MAX_WALLS {
            assert!(round.cross(Material::Glass, 0.01));
        }
        assert_eq!(round.reach(Material::Glass), 0.0);
        assert!(!round.cross(Material::Glass, 0.01));
        let mut pistol = Penetration::new(0.0);
        assert!(!pistol.cross(Material::Glass, 0.001));
    }

    #[test]
    fn reach_is_capped() {
        assert_eq!(Penetration::new(5.0).reach(Material::Wood), MAX_THICKNESS);
        let reach = Penetration::new(0.3).reach(Material::Concrete);
        assert!((reach - 0.1).abs() < 1e-6);
    }
}
