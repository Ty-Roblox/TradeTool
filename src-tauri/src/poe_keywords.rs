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
            description: "Armour reduces Physical damage taken from Hits. It is more effective against smaller Hits.",
            category: "defence",
        }),
        "bleed" => Some(PoeKeyword {
            label: "Bleed",
            description: "Bleeding is a Physical damaging Ailment that deals damage over time.",
            category: "ailment-debuff",
        }),
        "chaos" => Some(PoeKeyword {
            label: "Chaos",
            description: "Chaos is a damage type resisted by Chaos Resistance. Chaos damage removes twice as much Energy Shield.",
            category: "damage",
        }),
        "chill" => Some(PoeKeyword {
            label: "Chill",
            description: "Chill is a Cold Ailment that Slows the affected target.",
            category: "ailment-debuff",
        }),
        "cold" => Some(PoeKeyword {
            label: "Cold",
            description: "Cold is an Elemental damage type that can Chill and Freeze enemies.",
            category: "damage",
        }),
        "critical" | "criticalstrike" => Some(PoeKeyword {
            label: "Critical",
            description: "Critical Hits deal extra damage based on Critical Damage Bonus.",
            category: "combat",
        }),
        "curse" => Some(PoeKeyword {
            label: "Curse",
            description: "Curses significantly affect targets. A target can have one Curse by default. Magic/Rare/Unique monsters have 15/30/50% less Curse effect.",
            category: "ailment-debuff",
        }),
        "energyshield" => Some(PoeKeyword {
            label: "Energy Shield",
            description: "Energy Shield protects Life by taking damage first. It rapidly recharges after you stop losing Energy Shield for a short time.",
            category: "defence",
        }),
        "evasion" => Some(PoeKeyword {
            label: "Evasion",
            description: "Evasion gives a chance to avoid Attack Hits.",
            category: "defence",
        }),
        "fire" => Some(PoeKeyword {
            label: "Fire",
            description: "Fire is an Elemental damage type that can Ignite enemies.",
            category: "damage",
        }),
        "freeze" => Some(PoeKeyword {
            label: "Freeze",
            description: "Freeze is a Cold Ailment that prevents the affected target from acting.",
            category: "ailment-debuff",
        }),
        "ignite" => Some(PoeKeyword {
            label: "Ignite",
            description: "Ignite is a Fire Ailment that deals Fire damage over time.",
            category: "ailment-debuff",
        }),
        "life" => Some(PoeKeyword {
            label: "Life",
            description: "Life is your main survivability resource. You die when Life reaches zero.",
            category: "resource",
        }),
        "lightning" => Some(PoeKeyword {
            label: "Lightning",
            description: "Lightning is an Elemental damage type that can Shock enemies.",
            category: "damage",
        }),
        "mana" => Some(PoeKeyword {
            label: "Mana",
            description: "Mana is spent to use Skills unless another cost replaces it.",
            category: "resource",
        }),
        "minion" => Some(PoeKeyword {
            label: "Minion",
            description: "Minions are summoned Allies that accompany and fight alongside you. Persistent Minions reserve Spirit while active.",
            category: "entity",
        }),
        "physical" => Some(PoeKeyword {
            label: "Physical",
            description: "Physical is a damage type mitigated by Armour and Physical Damage Reduction.",
            category: "damage",
        }),
        "poison" => Some(PoeKeyword {
            label: "Poison",
            description: "Poison is a Chaos damaging Ailment that deals damage over time.",
            category: "ailment-debuff",
        }),
        "shock" => Some(PoeKeyword {
            label: "Shock",
            description: "Shock is a Lightning Ailment that causes the affected target to take increased damage.",
            category: "ailment-debuff",
        }),
        "stunthreshold" => Some(PoeKeyword {
            label: "Stun Threshold",
            description: "Stun Threshold is used to determine Stun buildup. Higher threshold makes stunning harder.",
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
