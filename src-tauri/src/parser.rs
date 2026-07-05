use crate::models::{CapturedItem, ItemModifier, ItemProperty};

pub fn parse_item_text(raw_text: &str) -> Result<CapturedItem, String> {
    let normalized = raw_text.replace("\r\n", "\n").replace('\r', "\n");
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return Err("Item text is empty.".to_string());
    }

    let sections = split_sections(&lines);
    let header = sections.first().ok_or_else(|| "Item text is missing a header.".to_string())?;
    let item_class = header_value(header, "Item Class:");
    let rarity = header_value(header, "Rarity:");
    let identity_lines = identity_lines(header);
    let (item_name, base_type) = identity_from_lines(rarity.as_deref(), &identity_lines);

    let mut item_level = None;
    let mut quality = None;
    let mut sockets = None;
    let mut properties = Vec::new();
    let mut explicit_mods = Vec::new();
    let mut collect_mods = false;

    for section in sections.iter().skip(1) {
        if section.iter().any(|line| line.starts_with("Item Level:")) {
            for line in section {
                if let Some(value) = line.strip_prefix("Item Level:") {
                    item_level = parse_first_i32(value).and_then(|value| u32::try_from(value).ok());
                }
            }
            collect_mods = true;
            continue;
        }

        if collect_mods {
            for line in section.iter().filter(|line| is_explicit_modifier_line(line)) {
                explicit_mods.push(ItemModifier {
                    index: explicit_mods.len(),
                    text: line.clone(),
                });
            }
            continue;
        }

        for line in section {
            if let Some(value) = line.strip_prefix("Quality:") {
                quality = parse_first_i32(value);
            }

            if let Some(value) = line.strip_prefix("Sockets:") {
                sockets = Some(value.trim().to_string());
            }

            if let Some((name, value)) = line.split_once(':') {
                properties.push(ItemProperty {
                    name: name.trim().to_string(),
                    value: value.trim().to_string(),
                });
            }
        }
    }

    Ok(CapturedItem {
        raw_text: normalized,
        item_class,
        rarity,
        item_name,
        base_type,
        item_level,
        quality,
        sockets,
        properties,
        explicit_mods,
    })
}

fn split_sections(lines: &[String]) -> Vec<Vec<String>> {
    let mut sections = Vec::new();
    let mut current = Vec::new();

    for line in lines {
        if line.chars().all(|ch| ch == '-') {
            if !current.is_empty() {
                sections.push(current);
                current = Vec::new();
            }
            continue;
        }

        current.push(line.clone());
    }

    if !current.is_empty() {
        sections.push(current);
    }

    sections
}

fn header_value(header: &[String], prefix: &str) -> Option<String> {
    header
        .iter()
        .find_map(|line| line.strip_prefix(prefix).map(str::trim).map(ToOwned::to_owned))
}

fn identity_lines(header: &[String]) -> Vec<String> {
    header
        .iter()
        .filter(|line| !line.starts_with("Item Class:") && !line.starts_with("Rarity:"))
        .cloned()
        .collect()
}

fn identity_from_lines(rarity: Option<&str>, lines: &[String]) -> (Option<String>, Option<String>) {
    match (rarity, lines) {
        (Some("Rare" | "Magic"), [item_name, base_type, ..]) => {
            (Some(item_name.clone()), Some(base_type.clone()))
        }
        (_, [base_type, ..]) => (None, Some(base_type.clone())),
        _ => (None, None),
    }
}

fn parse_first_i32(value: &str) -> Option<i32> {
    let number = value
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit() && *ch != '-')
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();

    number.parse().ok()
}

fn is_explicit_modifier_line(line: &str) -> bool {
    !line.contains(':')
        && !matches!(
            line,
            "Corrupted" | "Mirrored" | "Unidentified" | "Unmodifiable"
        )
}

#[cfg(test)]
mod tests {
    use super::parse_item_text;

    const RARE_BODY_ARMOUR: &str = "Item Class: Body Armours
Rarity: Rare
Dread Shelter
Expert Hexer's Robe
--------
Quality: +20% (augmented)
Energy Shield: 198 (augmented)
--------
Requirements:
Level: 65
Int: 157
--------
Sockets: S S
--------
Item Level: 72
--------
+78 to maximum Life
+34% to Fire Resistance
+29% to Lightning Resistance
15% increased Stun Threshold";

    #[test]
    fn parses_rare_gear_identity_properties_and_explicit_mods() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("rare body armour should parse");

        assert_eq!(item.item_class.as_deref(), Some("Body Armours"));
        assert_eq!(item.rarity.as_deref(), Some("Rare"));
        assert_eq!(item.item_name.as_deref(), Some("Dread Shelter"));
        assert_eq!(item.base_type.as_deref(), Some("Expert Hexer's Robe"));
        assert_eq!(item.item_level, Some(72));
        assert_eq!(item.quality, Some(20));
        assert_eq!(item.sockets.as_deref(), Some("S S"));
        assert!(item.explicit_mods.iter().any(|modifier| modifier.text == "+78 to maximum Life"));
        assert!(item.explicit_mods.iter().any(|modifier| modifier.text == "+34% to Fire Resistance"));
    }
}
