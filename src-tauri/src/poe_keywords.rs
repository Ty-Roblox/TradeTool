#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoeKeyword {
    pub label: &'static str,
    pub description: &'static str,
    pub category: &'static str,
}

pub fn describe_keyword(tag: &str) -> Option<PoeKeyword> {
    match keyword_key(tag).as_str() {
        "armour" => Some(PoeKeyword {
            label: "Armour",
            description: "A defence that reduces the physical damage taken from hits.",
            category: "defence",
        }),
        "bleed" => Some(PoeKeyword {
            label: "Bleed",
            description: "A damaging ailment that deals physical damage over time while active.",
            category: "ailment-debuff",
        }),
        "chaos" => Some(PoeKeyword {
            label: "Chaos",
            description: "A damage type resisted by Chaos Resistance and often associated with damage over time.",
            category: "damage",
        }),
        "chill" => Some(PoeKeyword {
            label: "Chill",
            description: "A cold ailment that slows the affected target.",
            category: "ailment-debuff",
        }),
        "cold" => Some(PoeKeyword {
            label: "Cold",
            description: "An elemental damage type that can chill or freeze enemies.",
            category: "damage",
        }),
        "critical" | "criticalstrike" => Some(PoeKeyword {
            label: "Critical",
            description: "Critical strikes are hits that deal extra damage based on critical damage bonus.",
            category: "combat",
        }),
        "curse" => Some(PoeKeyword {
            label: "Curse",
            description: "A debuff that applies harmful effects to enemies or players while the curse is active.",
            category: "ailment-debuff",
        }),
        "energyshield" => Some(PoeKeyword {
            label: "Energy Shield",
            description: "A protective resource that absorbs damage before Life until depleted.",
            category: "defence",
        }),
        "evasion" => Some(PoeKeyword {
            label: "Evasion",
            description: "A defence that gives a chance to avoid attack hits.",
            category: "defence",
        }),
        "fire" => Some(PoeKeyword {
            label: "Fire",
            description: "An elemental damage type that can ignite enemies.",
            category: "damage",
        }),
        "freeze" => Some(PoeKeyword {
            label: "Freeze",
            description: "A cold ailment that prevents the affected target from acting for its duration.",
            category: "ailment-debuff",
        }),
        "ignite" => Some(PoeKeyword {
            label: "Ignite",
            description: "A fire ailment that deals burning damage over time.",
            category: "ailment-debuff",
        }),
        "life" => Some(PoeKeyword {
            label: "Life",
            description: "Your main survivability resource. You die when Life reaches zero.",
            category: "resource",
        }),
        "lightning" => Some(PoeKeyword {
            label: "Lightning",
            description: "An elemental damage type that can shock enemies.",
            category: "damage",
        }),
        "mana" => Some(PoeKeyword {
            label: "Mana",
            description: "A resource spent to use skills unless another cost replaces it.",
            category: "resource",
        }),
        "minion" => Some(PoeKeyword {
            label: "Minion",
            description: "An allied summoned entity affected by modifiers that specifically apply to minions.",
            category: "entity",
        }),
        "physical" => Some(PoeKeyword {
            label: "Physical",
            description: "A damage type mitigated primarily by armour and physical damage reduction.",
            category: "damage",
        }),
        "poison" => Some(PoeKeyword {
            label: "Poison",
            description: "A chaos ailment that deals chaos damage over time based on the hit that inflicted it.",
            category: "ailment-debuff",
        }),
        "shock" => Some(PoeKeyword {
            label: "Shock",
            description: "A lightning ailment that causes the affected target to take increased damage.",
            category: "ailment-debuff",
        }),
        "stunthreshold" => Some(PoeKeyword {
            label: "Stun Threshold",
            description: "A value used when determining whether a hit can stun a target.",
            category: "defence",
        }),
        _ => None,
    }
}

fn keyword_key(tag: &str) -> String {
    tag.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
