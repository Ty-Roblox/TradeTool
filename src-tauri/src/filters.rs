use crate::models::{CapturedItem, FilterCandidate, FilterGroup};
use crate::trade::{mapped_explicit_modifier_indices, trade_filter_specs};

pub fn generate_filter_groups(item: &CapturedItem) -> Vec<FilterGroup> {
    let mut groups = Vec::new();

    let trade_filters = trade_filter_specs(item)
        .into_iter()
        .map(|spec| FilterCandidate {
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
            selected_by_default: false,
            supported: true,
            unsupported_reason: None,
        });
    }
    if let Some(rarity) = &item.rarity {
        identity.push(FilterCandidate {
            id: "identity:rarity".to_string(),
            label: format!("Rarity: {rarity}"),
            selected_by_default: false,
            supported: true,
            unsupported_reason: None,
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
    if let Some(item_level) = item.item_level {
        misc.push(FilterCandidate {
            id: "misc:item_level".to_string(),
            label: format!("Item level: {item_level}+"),
            selected_by_default: false,
            supported: true,
            unsupported_reason: None,
        });
    }
    if let Some(quality) = item.quality {
        misc.push(FilterCandidate {
            id: "property:quality".to_string(),
            label: format!("Quality: {quality}%+"),
            selected_by_default: false,
            supported: true,
            unsupported_reason: None,
        });
    }
    if let Some(sockets) = &item.sockets {
        misc.push(FilterCandidate {
            id: "property:sockets".to_string(),
            label: format!("Sockets: {sockets}"),
            selected_by_default: false,
            supported: false,
            unsupported_reason: Some("Socket filters need POE2-specific trade mapping.".to_string()),
        });
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
            .map(|modifier| FilterCandidate {
                id: format!("explicit:{}", modifier.index),
                label: modifier.text.clone(),
                selected_by_default: false,
                supported: false,
                unsupported_reason: Some(
                    "Modifier stat ID mapping will expand from real POE2 fixtures.".to_string(),
                ),
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

#[cfg(test)]
mod tests {
    use crate::filters::generate_filter_groups;
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

    #[test]
    fn creates_stable_filter_candidates_for_parsed_item() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("item should parse");
        let groups = generate_filter_groups(&item);
        let filters = groups
            .iter()
            .flat_map(|group| group.filters.iter())
            .collect::<Vec<_>>();

        assert!(filters.iter().any(|filter| filter.id == "identity:type" && !filter.selected_by_default));
        assert!(filters.iter().any(|filter| filter.id == "misc:item_level" && !filter.selected_by_default));
        assert!(filters.iter().any(|filter| filter.id == "property:quality" && !filter.selected_by_default));

        let life_filter = filters
            .iter()
            .find(|filter| filter.id == "explicit:0")
            .expect("explicit modifier candidate");
        assert_eq!(life_filter.label, "+78 to maximum Life");
        assert!(!life_filter.supported);
    }
}
