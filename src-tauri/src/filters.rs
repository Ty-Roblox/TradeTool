use crate::models::{AppDiagnostic, CapturedItem, FilterCandidate, FilterGroup};
use crate::trade::{
    gem_level, is_gem_item_class, mapped_explicit_modifier_indices,
    should_show_unsupported_modifier, socket_count, trade_filter_specs,
};

pub fn generate_filter_groups(item: &CapturedItem) -> Vec<FilterGroup> {
    let mut groups = Vec::new();
    let is_gem = is_gem_item_class(item.item_class.as_deref());

    let trade_filters = trade_filter_specs(item)
        .into_iter()
        .map(|spec| FilterCandidate {
            default_min: spec.default_min(),
            default_max: spec.default_max(),
            id: spec.id,
            label: spec.label,
            selected_by_default: spec.selected_by_default,
            supported: true,
            unsupported_reason: None,
        })
        .collect::<Vec<_>>();

    if !trade_filters.is_empty() {
        groups.push(FilterGroup {
            id: "trade".to_string(),
            label: "Trade Query".to_string(),
            filters: trade_filters,
        });
    }

    let mut identity = Vec::new();
    if let Some(base_type) = &item.base_type {
        identity.push(FilterCandidate {
            id: "identity:type".to_string(),
            label: format!("Base type: {base_type}"),
            selected_by_default: is_gem,
            supported: true,
            unsupported_reason: None,
            default_min: None,
            default_max: None,
        });
    }
    if let Some(rarity) = &item.rarity {
        identity.push(FilterCandidate {
            id: "identity:rarity".to_string(),
            label: format!("Rarity: {rarity}"),
            selected_by_default: is_gem,
            supported: true,
            unsupported_reason: None,
            default_min: None,
            default_max: None,
        });
    }
    if !identity.is_empty() {
        groups.push(FilterGroup {
            id: "identity".to_string(),
            label: "Identity".to_string(),
            filters: identity,
        });
    }

    let mut misc = Vec::new();
    if let Some(level) = gem_level(item) {
        misc.push(FilterCandidate {
            id: "property:gem_level".to_string(),
            label: format!("Gem level: {level}+"),
            selected_by_default: true,
            supported: true,
            unsupported_reason: None,
            default_min: Some(level as f64),
            default_max: None,
        });
    }
    if let Some(item_level) = item.item_level {
        misc.push(FilterCandidate {
            id: "misc:item_level".to_string(),
            label: format!("Item level: {item_level}+"),
            selected_by_default: false,
            supported: true,
            unsupported_reason: None,
            default_min: Some(item_level as f64),
            default_max: None,
        });
    }
    if let Some(quality) = item.quality {
        misc.push(FilterCandidate {
            id: "property:quality".to_string(),
            label: format!("Quality: {quality}%+"),
            selected_by_default: is_gem,
            supported: true,
            unsupported_reason: None,
            default_min: Some(quality as f64),
            default_max: None,
        });
    }
    if let Some(sockets) = &item.sockets {
        match socket_count(sockets) {
            Some(count) => misc.push(FilterCandidate {
                id: "property:sockets".to_string(),
                label: format!("Sockets: {count}+ ({sockets})"),
                selected_by_default: false,
                supported: true,
                unsupported_reason: None,
                default_min: Some(count as f64),
                default_max: None,
            }),
            None => misc.push(FilterCandidate {
                id: "property:sockets".to_string(),
                label: format!("Sockets: {sockets}"),
                selected_by_default: false,
                supported: false,
                unsupported_reason: Some(
                    "Socket filters need POE2-specific trade mapping.".to_string(),
                ),
                default_min: None,
                default_max: None,
            }),
        }
    }
    if !misc.is_empty() {
        groups.push(FilterGroup {
            id: "misc".to_string(),
            label: "Item Details".to_string(),
            filters: misc,
        });
    }

    if !item.explicit_mods.is_empty() {
        let mapped_indices = mapped_explicit_modifier_indices(item);
        let unsupported_mods = item
            .explicit_mods
            .iter()
            .filter(|modifier| !mapped_indices.contains(&modifier.index))
            .filter(|modifier| should_show_unsupported_modifier(&modifier.text))
            .map(|modifier| FilterCandidate {
                id: format!("explicit:{}", modifier.index),
                label: modifier.text.clone(),
                selected_by_default: false,
                supported: false,
                unsupported_reason: Some(
                    "Modifier stat ID mapping will expand from real POE2 fixtures.".to_string(),
                ),
                default_min: None,
                default_max: None,
            })
            .collect::<Vec<_>>();

        if unsupported_mods.is_empty() {
            return groups;
        }

        groups.push(FilterGroup {
            id: "explicit".to_string(),
            label: "Explicit Modifiers".to_string(),
            filters: unsupported_mods,
        });
    }

    groups
}

pub fn generate_capture_diagnostics(groups: &[FilterGroup]) -> Vec<AppDiagnostic> {
    groups
        .iter()
        .flat_map(|group| group.filters.iter())
        .filter(|filter| !filter.supported)
        .map(|filter| {
            let code = if filter.id.starts_with("explicit:") {
                "unmapped_modifier"
            } else {
                "unsupported_filter"
            };
            let detail = match filter.unsupported_reason.as_deref() {
                Some(reason) if !reason.trim().is_empty() => {
                    Some(format!("{}: {reason}", filter.label))
                }
                _ => Some(filter.label.clone()),
            };

            AppDiagnostic {
                code: code.to_string(),
                message: format!("Failed filter id {}", filter.id),
                detail,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::filters::{generate_capture_diagnostics, generate_filter_groups};
    use crate::parser::parse_item_text;

    const RARE_BODY_ARMOUR: &str = "Item Class: Body Armours
Rarity: Rare
Dread Shelter
Expert Hexer's Robe
--------
Quality: +20%
--------
Item Level: 72
--------
+78 to maximum Life";

    const UNIQUE_WITH_BOILERPLATE_MODS: &str = "Item Class: Body Armours
Rarity: Unique
Trial Shelter
Expert Hexer's Robe
--------
Item Level: 82
--------
{ Enhancement }
Allocates Zarokh's Gift -- Unscalable Value
{ Implicit Modifier }
40(39-44)% increased Evasion Rating
+114(100-119) to maximum Life
Darkness howls through ancient bones, a wistful cry";

    const RARE_WITH_UNMAPPED_MOD: &str = "Item Class: Body Armours
Rarity: Rare
Dread Shelter
Expert Hexer's Robe
--------
Item Level: 72
--------
123% made up local nonsense";

    const RARE_WITH_SOCKETS_AND_REDUCED_POISON_DURATION: &str = "Item Class: Boots
Rarity: Rare
Plague Slippers
Bound Sandals
--------
Sockets: S S
--------
Item Level: 72
--------
59(60-56)% reduced Poison Duration on you";

    const RARE_BELT_WITH_CHARM_SLOTS: &str = "Item Class: Belts
Rarity: Rare
Binding Buckle
Mail Belt
--------
Item Level: 82
--------
+114(100-119) to maximum Life
+17(17-18)% to all Elemental Resistances
+2 to Level of all Melee Skills
Has 2(1-3) Charm Slots";

    const ACTIVE_SKILL_GEM: &str = "Item Class: Skill Gems
Rarity: Gem
Spark
--------
Level: 15
Quality: +20%
--------
Item Level: 15";

    #[test]
    fn creates_stable_filter_candidates_for_parsed_item() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("item should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();

        assert!(filters
            .iter()
            .any(|filter| filter.id == "identity:type" && !filter.selected_by_default));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "misc:item_level" && !filter.selected_by_default));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "property:quality" && !filter.selected_by_default));
        assert!(filters.iter().any(|filter| {
            filter.id == "misc:exact_selected_explicit_affixes"
                && filter.supported
                && !filter.selected_by_default
        }));

        let life_filter = filters
            .iter()
            .find(|filter| filter.id == "stat:explicit.stat_3299347043:0")
            .expect("maximum life trade candidate");
        assert!(life_filter.label.contains("maximum Life"));
        assert!(life_filter.supported);
        assert!(life_filter.selected_by_default);
    }

    #[test]
    fn creates_gem_base_rarity_category_and_level_filters() {
        let item = parse_item_text(ACTIVE_SKILL_GEM).expect("skill gem should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();

        assert!(filters.iter().any(|filter| {
            filter.id == "category:gem.activegem" && filter.supported && filter.selected_by_default
        }));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "identity:type" && filter.selected_by_default));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "identity:rarity" && filter.selected_by_default));

        let gem_level = filters
            .iter()
            .find(|filter| filter.id == "property:gem_level")
            .expect("gem level filter");
        assert_eq!(gem_level.default_min, Some(15.0));
        assert!(gem_level.selected_by_default);

        let quality = filters
            .iter()
            .find(|filter| filter.id == "property:quality")
            .expect("quality filter");
        assert_eq!(quality.default_min, Some(20.0));
        assert!(quality.selected_by_default);
    }

    #[test]
    fn hides_non_searchable_boilerplate_from_unsupported_modifiers() {
        let item =
            parse_item_text(UNIQUE_WITH_BOILERPLATE_MODS).expect("unique body armour should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();
        let labels = filters
            .iter()
            .map(|filter| filter.label.as_str())
            .collect::<Vec<_>>();

        assert!(filters
            .iter()
            .any(|filter| filter.id == "stat:explicit.stat_124859000:3"));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "stat:explicit.stat_3299347043:4"));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "stat:explicit.stat_2954116742|11184:1"));
        assert!(!labels.contains(&"{ Enhancement }"));
        assert!(!labels.contains(&"{ Implicit Modifier }"));
        assert!(!labels.iter().any(|label| label.contains("wistful cry")));
    }

    #[test]
    fn emits_diagnostics_for_failed_filter_ids() {
        let item = parse_item_text(RARE_WITH_UNMAPPED_MOD).expect("item should parse");
        let groups = generate_filter_groups(&item);
        let diagnostics = generate_capture_diagnostics(&groups);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unmapped_modifier"
                && diagnostic.message.contains("explicit:0")
                && diagnostic
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("123% made up local nonsense"))
        }));
    }

    #[test]
    fn sockets_and_reduced_poison_duration_are_supported_filters() {
        let item = parse_item_text(RARE_WITH_SOCKETS_AND_REDUCED_POISON_DURATION)
            .expect("item should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();
        let diagnostics = generate_capture_diagnostics(&groups);

        assert!(filters.iter().any(|filter| {
            filter.id == "property:sockets" && filter.supported && filter.label.contains("2+")
        }));
        assert!(filters
            .iter()
            .any(|filter| filter.id == "stat:explicit.stat_3301100256:0" && filter.supported));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ranged_charm_slots_are_supported_filters() {
        let item = parse_item_text(RARE_BELT_WITH_CHARM_SLOTS).expect("item should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();
        let diagnostics = generate_capture_diagnostics(&groups);

        assert!(filters.iter().any(|filter| {
            filter.id == "stat:explicit.stat_1416292992:3"
                && filter.supported
                && filter.selected_by_default
                && filter.label.contains("Charm Slot")
        }));
        assert!(diagnostics.is_empty());
    }
}
