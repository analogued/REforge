// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Optional development controls. This module is absent from normal builds.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Actor {
    AlleyDog,
    RoomDog,
    WindowProjectiles,
    MilkDogs,
    Spider,
    Eels,
    BinCat,
    AlleyMice,
    Broom,
    HeartsArrows,
    HeartsRivals,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Damage {
    AlleyDog,
    RoomDog,
    WindowProjectiles,
    MilkDogs,
    Spider,
    Eels,
    Air,
}

const ACTORS: &[(&str, Actor)] = &[
    ("alley-dog", Actor::AlleyDog),
    ("room-dog", Actor::RoomDog),
    ("window-projectiles", Actor::WindowProjectiles),
    ("milk-dogs", Actor::MilkDogs),
    ("spider", Actor::Spider),
    ("eels", Actor::Eels),
    ("bin-cat", Actor::BinCat),
    ("alley-mice", Actor::AlleyMice),
    ("broom", Actor::Broom),
    ("hearts-arrows", Actor::HeartsArrows),
    ("hearts-rivals", Actor::HeartsRivals),
];

const DAMAGE: &[(&str, Damage)] = &[
    ("alley-dog", Damage::AlleyDog),
    ("room-dog", Damage::RoomDog),
    ("window-projectiles", Damage::WindowProjectiles),
    ("milk-dogs", Damage::MilkDogs),
    ("spider", Damage::Spider),
    ("eels", Damage::Eels),
    ("air", Damage::Air),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DevOptions {
    disabled_actors: u16,
    disabled_damage: u16,
    /// Reserved until life loss and re-entry have been implemented.
    pub infinite_lives: bool,
    /// Restore the original one-tick window hold at internal difficulty six.
    pub original_window_timing: bool,
}

fn parse_mask<T: Copy>(value: &str, choices: &[(&str, T)], kind: &str) -> Result<u16, String> {
    match value.trim() {
        "all" => return Ok((1 << choices.len()) - 1),
        "none" => return Ok(0),
        _ => {}
    }
    let mut mask = 0;
    for name in value.split(',').map(str::trim) {
        let index = choices.iter().position(|(choice, _)| *choice == name);
        let Some(index) = index else {
            let names = choices.iter().map(|(name, _)| *name).collect::<Vec<_>>();
            return Err(format!(
                "Unknown {kind}: {name:?}; use all, none or {}",
                names.join(",")
            ));
        };
        mask |= 1 << index;
    }
    Ok(mask)
}

impl DevOptions {
    /// Each occurrence replaces the previous selection; malformed lists leave
    /// the existing mask unchanged. This allows overriding launcher defaults.
    pub fn set_disabled_actors(&mut self, value: &str) -> Result<(), String> {
        self.disabled_actors = parse_mask(value, ACTORS, "actor")?;
        Ok(())
    }

    pub fn set_disabled_damage(&mut self, value: &str) -> Result<(), String> {
        self.disabled_damage = parse_mask(value, DAMAGE, "damage source")?;
        Ok(())
    }

    pub fn actor_disabled(self, actor: Actor) -> bool {
        self.disabled_actors & (1 << actor as usize) != 0
    }

    pub fn damage_disabled(self, source: Damage) -> bool {
        self.disabled_damage & (1 << source as usize) != 0
    }

    /// Report requested controls which cannot affect the incomplete game yet.
    pub fn stub_notice(self) -> Option<String> {
        let mut controls = Vec::new();
        if self.disabled_damage != 0 {
            controls.push("damage overrides (death behavior is not implemented)");
        }
        if self.infinite_lives {
            controls.push("infinite lives (life loss is not implemented)");
        }
        for (name, actor) in &ACTORS[..3] {
            if self.actor_disabled(*actor) {
                controls.push(name);
            }
        }
        (!controls.is_empty()).then(|| {
            format!(
                "Development placeholders with no effect yet: {}.",
                controls.join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_and_damage_masks_are_independent_and_launcher_defaults_can_be_replaced() {
        let mut options = DevOptions::default();
        options.set_disabled_damage("all").unwrap();
        options.set_disabled_actors("alley-dog, eels").unwrap();
        options.set_disabled_damage("air,eels").unwrap();
        assert!(options.actor_disabled(Actor::AlleyDog));
        assert!(options.actor_disabled(Actor::Eels));
        assert!(!options.actor_disabled(Actor::Spider));
        assert!(!options.damage_disabled(Damage::AlleyDog));
        assert!(options.damage_disabled(Damage::Eels));
        assert!(options.damage_disabled(Damage::Air));
        options.set_disabled_damage("none").unwrap();
        assert!(!options.damage_disabled(Damage::Air));
        options.set_disabled_actors("none").unwrap();
        assert!(!options.actor_disabled(Actor::AlleyDog));
    }

    #[test]
    fn malformed_lists_do_not_partially_change_masks() {
        let mut options = DevOptions::default();
        options.set_disabled_actors("broom").unwrap();
        options.set_disabled_damage("spider").unwrap();
        let previous = options;
        for value in [
            "",
            ",",
            "all,eels",
            "none,eels",
            "eel",
            "eels,",
            "eels,nope",
        ] {
            assert!(options.set_disabled_actors(value).is_err());
            assert!(options.set_disabled_damage(value).is_err());
            assert_eq!(options, previous);
        }
    }

    #[test]
    fn all_covers_each_actor_and_source_and_notices_only_describe_placeholders() {
        let mut options = DevOptions::default();
        assert!(options.stub_notice().is_none());
        options.set_disabled_actors("all").unwrap();
        assert!(
            ACTORS
                .iter()
                .all(|(_, actor)| options.actor_disabled(*actor))
        );
        options.set_disabled_damage("all").unwrap();
        assert!(
            DAMAGE
                .iter()
                .all(|(_, source)| options.damage_disabled(*source))
        );
        options.infinite_lives = true;
        let notice = options.stub_notice().unwrap();
        assert!(notice.contains("life loss is not implemented"));
        assert!(notice.contains("alley-dog"));
        options = DevOptions::default();
        options.set_disabled_actors("bin-cat,broom,eels").unwrap();
        assert!(options.stub_notice().is_none());
    }
}
