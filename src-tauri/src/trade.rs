use std::collections::HashSet;

use crate::models::{
    CapturedItem, TradeListing, TradeListingItem, TradePrice, TradeSearchResponse,
};
use serde::Deserialize;
use serde_json::{json, Value};

const TRADE_BASE_URL: &str = "https://www.pathofexile.com";
const FETCH_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct TradeFilterSpec {
    pub id: String,
    pub label: String,
    pub selected_by_default: bool,
    pub source_modifier_index: Option<usize>,
    kind: TradeFilterKind,
}

#[derive(Debug, Clone, PartialEq)]
enum TradeFilterKind {
    Category(String),
    Stat { stat_id: String, value: f64 },
}

pub fn trade_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let mut specs = Vec::new();

    if let Some(category) = item.item_class.as_deref().and_then(category_for_item_class) {
        specs.push(TradeFilterSpec {
            id: format!("category:{category}"),
            label: format!("Category: {}", item.item_class.as_deref().unwrap_or("Item")),
            selected_by_default: true,
            source_modifier_index: None,
            kind: TradeFilterKind::Category(category.to_string()),
        });
    }

    specs.extend(stat_filter_specs(item));
    specs
}

pub fn mapped_explicit_modifier_indices(item: &CapturedItem) -> HashSet<usize> {
    let mut indices = stat_filter_specs(item)
        .into_iter()
        .filter_map(|spec| spec.source_modifier_index)
        .collect::<HashSet<_>>();

    for modifier in &item.explicit_mods {
        if is_elemental_resistance_modifier(&normalized_modifier_text(&modifier.text)) {
            indices.insert(modifier.index);
        }
    }

    indices
}

pub fn build_trade_query(item: &CapturedItem, selected_filter_ids: &[String]) -> Result<Value, String> {
    let selected = selected_filter_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let trade_specs = trade_filter_specs(item);
    let selected_specs = trade_specs
        .iter()
        .filter(|spec| selected.contains(spec.id.as_str()))
        .collect::<Vec<_>>();
    let stat_filters = selected_specs
        .iter()
        .filter_map(|spec| match &spec.kind {
            TradeFilterKind::Stat { stat_id, value } => Some(json!({
                "id": stat_id,
                "disabled": false,
                "value": { "min": stat_value_json(*value) }
            })),
            TradeFilterKind::Category(_) => None,
        })
        .collect::<Vec<_>>();

    let mut query = json!({
        "query": {
            "status": { "option": "any" },
            "stats": []
        },
        "sort": {
            "price": "asc"
        }
    });

    if !stat_filters.is_empty() {
        query["query"]["stats"] = json!([
            {
                "type": "and",
                "filters": stat_filters,
                "disabled": false
            }
        ]);
    }

    for spec in &selected_specs {
        if let TradeFilterKind::Category(category) = &spec.kind {
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"] = json!(category);
            query["query"]["filters"]["type_filters"]["disabled"] = json!(false);
        }
    }

    if selected.contains("identity:type") {
        if let Some(base_type) = &item.base_type {
            query["query"]["type"] = json!(base_type);
        }
    }

    if selected.contains("identity:rarity") {
        if let Some(rarity) = &item.rarity {
            query["query"]["filters"]["type_filters"]["filters"]["rarity"]["option"] =
                json!(rarity.to_ascii_lowercase());
        }
    }

    if selected.contains("misc:item_level") {
        if let Some(item_level) = item.item_level {
            query["query"]["filters"]["misc_filters"]["filters"]["ilvl"]["min"] = json!(item_level);
        }
    }

    if selected.contains("property:quality") {
        if let Some(quality) = item.quality {
            query["query"]["filters"]["misc_filters"]["filters"]["quality"]["min"] = json!(quality);
        }
    }

    query["query"]["filters"]["trade_filters"]["filters"] = json!({});

    Ok(query)
}

pub fn selected_pseudo_stat_ids(item: &CapturedItem, selected_filter_ids: &[String]) -> Vec<String> {
    let selected = selected_filter_ids.iter().map(String::as_str).collect::<HashSet<_>>();

    trade_filter_specs(item)
        .into_iter()
        .filter(|spec| selected.contains(spec.id.as_str()))
        .filter_map(|spec| match spec.kind {
            TradeFilterKind::Stat { stat_id, .. } if stat_id.starts_with("pseudo.") => Some(stat_id),
            _ => None,
        })
        .collect()
}

pub fn build_fetch_url(
    search_id: &str,
    result_ids: &[String],
    pseudo_stat_ids: &[String],
) -> Result<String, String> {
    if search_id.trim().is_empty() {
        return Err("Search id is required to fetch listings.".to_string());
    }

    if result_ids.is_empty() {
        return Err("At least one result id is required to fetch listings.".to_string());
    }

    let ids = result_ids.join(",");
    let mut url = format!("{TRADE_BASE_URL}/api/trade2/fetch/{ids}?query={search_id}&realm=poe2");

    for pseudo in pseudo_stat_ids {
        url.push_str("&pseudos[]=");
        url.push_str(pseudo);
    }

    Ok(url)
}

pub fn map_fetch_response(response_body: &str) -> Result<Vec<TradeListing>, String> {
    let response = serde_json::from_str::<TradeFetchApiResponse>(response_body)
        .map_err(|error| format!("POE2 trade fetch response was not understood: {error}"))?;

    Ok(response
        .result
        .into_iter()
        .map(|result| TradeListing {
            id: result.id,
            indexed: result.listing.indexed,
            price: result.listing.price.and_then(|price| {
                Some(TradePrice {
                    price_type: price.price_type,
                    amount: price.amount?,
                    currency: price.currency?,
                })
            }),
            account_name: result.listing.account.and_then(|account| account.name),
            item: TradeListingItem {
                icon: result.item.icon,
                name: result.item.name.filter(|name| !name.trim().is_empty()),
                type_line: result.item.type_line,
                base_type: result.item.base_type,
                rarity: result.item.rarity,
                item_level: result.item.item_level,
                explicit_mods: result
                    .item
                    .explicit_mods
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(FetchMod::into_text)
                    .collect(),
                pseudo_mods: result.item.pseudo_mods.unwrap_or_default(),
            },
        })
        .collect())
}

pub async fn search_trade(
    league: &str,
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> Result<TradeSearchResponse, String> {
    let league = sanitize_league(league)?;
    let query = build_trade_query(item, selected_filter_ids)?;
    let api_url = format!("{TRADE_BASE_URL}/api/trade2/search/poe2/{league}");

    let response = reqwest::Client::new()
        .post(&api_url)
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .header("user-agent", "TradeProject/0.1.0")
        .json(&query)
        .send()
        .await
        .map_err(|error| format!("Could not reach the POE2 trade API: {error}"))?;

    let status = response.status();
    if is_blocked_or_rate_limited(status.as_u16()) {
        return Err(
            "The POE2 trade API blocked or rate-limited the request. Open the trade site in a browser and try again later."
                .to_string(),
        );
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("POE2 trade API returned {status}: {}", body.trim()));
    }

    let body = response
        .json::<TradeSearchApiResponse>()
        .await
        .map_err(|error| format!("POE2 trade API response was not understood: {error}"))?;

    if body.id.trim().is_empty() {
        return Err("POE2 trade API did not return a search id.".to_string());
    }

    let url = format!("{TRADE_BASE_URL}/trade2/search/poe2/{league}/{}", body.id);
    let result_ids = body.result;
    let first_page_ids = result_ids
        .iter()
        .take(FETCH_PAGE_SIZE)
        .cloned()
        .collect::<Vec<_>>();
    let pseudo_stat_ids = selected_pseudo_stat_ids(item, selected_filter_ids);
    let mut listings = Vec::new();
    let mut fetch_url = None;
    let mut warning = None;

    if first_page_ids.is_empty() {
        warning = Some("The POE2 trade API returned no matching listings.".to_string());
    } else {
        let url = build_fetch_url(&body.id, &first_page_ids, &pseudo_stat_ids)?;
        fetch_url = Some(url.clone());

        match fetch_trade_listings(&url).await {
            Ok(fetched) => listings = fetched,
            Err(error) => warning = Some(error),
        }
    }

    Ok(TradeSearchResponse {
        url,
        search_id: body.id,
        total: body.total.unwrap_or(result_ids.len()),
        result_ids,
        fetched_count: listings.len(),
        listings,
        query,
        fetch_url,
        warning,
    })
}

pub fn validate_trade_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    let allowed_prefix = format!("{TRADE_BASE_URL}/trade2/search/poe2/");

    if trimmed.starts_with(&allowed_prefix)
        && !trimmed.chars().any(char::is_control)
        && !trimmed.contains(char::is_whitespace)
    {
        Ok(trimmed.to_string())
    } else {
        Err("Only official POE2 trade search URLs can be opened.".to_string())
    }
}

async fn fetch_trade_listings(fetch_url: &str) -> Result<Vec<TradeListing>, String> {
    let response = reqwest::Client::new()
        .get(fetch_url)
        .header("accept", "application/json")
        .header("user-agent", "TradeProject/0.1.0")
        .send()
        .await
        .map_err(|error| format!("Could not fetch POE2 trade listings: {error}"))?;

    let status = response.status();
    if is_blocked_or_rate_limited(status.as_u16()) {
        return Err(
            "The POE2 trade API blocked or rate-limited listing fetches. Open the official search page and try again later."
                .to_string(),
        );
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("POE2 trade listing fetch returned {status}: {}", body.trim()));
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read POE2 trade listing response: {error}"))?;

    map_fetch_response(&body)
}

fn sanitize_league(league: &str) -> Result<String, String> {
    let trimmed = league.trim();
    if trimmed.is_empty() {
        return Err("League is required.".to_string());
    }

    Ok(trimmed.replace(' ', "%20"))
}

pub fn is_blocked_or_rate_limited(status: u16) -> bool {
    status == 403 || status == 429
}

fn stat_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let mut keyed_specs = Vec::new();
    let mut elemental_resistance_total = 0.0;
    let mut first_resistance_index = None;

    for modifier in &item.explicit_mods {
        let text = normalized_modifier_text(&modifier.text);

        if let Some(value) = mapped_explicit_stat_value(&text, "to maximum Energy Shield") {
            keyed_specs.push((
                modifier.index,
                TradeFilterSpec {
                    id: format!("stat:explicit.stat_4052037485:{}", modifier.index),
                    label: format!("Maximum Energy Shield: {}+", format_filter_value(value)),
                    selected_by_default: true,
                    source_modifier_index: Some(modifier.index),
                    kind: TradeFilterKind::Stat {
                        stat_id: "explicit.stat_4052037485".to_string(),
                        value,
                    },
                },
            ));
        } else if let Some(value) = mapped_explicit_stat_value(&text, "increased Energy Shield") {
            keyed_specs.push((
                modifier.index,
                TradeFilterSpec {
                    id: format!("stat:explicit.stat_4015621042:{}", modifier.index),
                    label: format!("Increased Energy Shield: {}%+", format_filter_value(value)),
                    selected_by_default: true,
                    source_modifier_index: Some(modifier.index),
                    kind: TradeFilterKind::Stat {
                        stat_id: "explicit.stat_4015621042".to_string(),
                        value,
                    },
                },
            ));
        } else if let Some(value) =
            mapped_explicit_stat_value(&text, "increased Rarity of Items found")
        {
            keyed_specs.push((
                modifier.index,
                TradeFilterSpec {
                    id: format!("stat:explicit.stat_3917489142:{}", modifier.index),
                    label: format!("Rarity of Items: {}%+", format_filter_value(value)),
                    selected_by_default: true,
                    source_modifier_index: Some(modifier.index),
                    kind: TradeFilterKind::Stat {
                        stat_id: "explicit.stat_3917489142".to_string(),
                        value,
                    },
                },
            ));
        } else if let Some(value) = mapped_explicit_stat_value(&text, "to Stun Threshold") {
            keyed_specs.push((
                modifier.index,
                TradeFilterSpec {
                    id: format!("stat:explicit.stat_915769802:{}", modifier.index),
                    label: format!("Stun Threshold: {}+", format_filter_value(value)),
                    selected_by_default: true,
                    source_modifier_index: Some(modifier.index),
                    kind: TradeFilterKind::Stat {
                        stat_id: "explicit.stat_915769802".to_string(),
                        value,
                    },
                },
            ));
        } else if let Some((stat_id, label, value)) = mapped_general_stat(&text) {
            keyed_specs.push((
                modifier.index,
                TradeFilterSpec {
                    id: format!("stat:{stat_id}:{}", modifier.index),
                    label,
                    selected_by_default: true,
                    source_modifier_index: Some(modifier.index),
                    kind: TradeFilterKind::Stat { stat_id, value },
                },
            ));
        }

        if is_elemental_resistance_modifier(&text) {
            if let Some(value) = parse_first_number(&text) {
                elemental_resistance_total += value;
                first_resistance_index.get_or_insert(modifier.index);
            }
        }
    }

    if elemental_resistance_total > 0.0 {
        keyed_specs.push((
            first_resistance_index.unwrap_or(usize::MAX),
            TradeFilterSpec {
                id: "stat:pseudo.pseudo_total_elemental_resistance".to_string(),
                label: format!(
                    "Total Elemental Resistance: {}%+",
                    format_filter_value(elemental_resistance_total)
                ),
                selected_by_default: true,
                source_modifier_index: first_resistance_index,
                kind: TradeFilterKind::Stat {
                    stat_id: "pseudo.pseudo_total_elemental_resistance".to_string(),
                    value: elemental_resistance_total,
                },
            },
        ));
    }

    keyed_specs.sort_by_key(|(index, _)| *index);
    keyed_specs.into_iter().map(|(_, spec)| spec).collect()
}

fn mapped_explicit_stat_value(text: &str, marker: &str) -> Option<f64> {
    text.contains(marker).then(|| parse_first_number(text)).flatten()
}

fn mapped_general_stat(text: &str) -> Option<(String, String, f64)> {
    if text.contains("Allies in your Presence have") && text.contains("increased Attack Speed") {
        let value = parse_first_number(text)?;
        let prefix = stat_source_prefix(text);
        return Some((
            format!("{prefix}.stat_1998951374"),
            format!("Allies Attack Speed: {}%+", format_filter_value(value)),
            value,
        ));
    }

    if text.contains("increased Spirit") {
        let value = parse_first_number(text)?;
        let prefix = stat_source_prefix(text);
        return Some((
            format!("{prefix}.stat_3984865854"),
            format!("Spirit: {}%+", format_filter_value(value)),
            value,
        ));
    }

    if text.contains("Allies in your Presence Regenerate") && text.contains("Life per second") {
        let value = parse_first_number(text)?;
        return Some((
            "explicit.stat_4010677958".to_string(),
            format!("Allies Life Regen: {}+", format_filter_value(value)),
            value,
        ));
    }

    if text.contains("to all Attributes") {
        let value = parse_first_number(text)?;
        let prefix = stat_source_prefix(text);
        return Some((
            format!("{prefix}.stat_1379411836"),
            format!("All Attributes: {}+", format_filter_value(value)),
            value,
        ));
    }

    if text.contains("Companions deal") && text.contains("increased damage to your Marked targets") {
        let value = parse_first_number(text)?;
        return Some((
            "explicit.stat_1067622524".to_string(),
            format!("Companion Marked Damage: {}%+", format_filter_value(value)),
            value,
        ));
    }

    None
}

fn stat_source_prefix(text: &str) -> &'static str {
    if text.contains("(rune)") {
        "rune"
    } else {
        "explicit"
    }
}

fn parse_first_number(value: &str) -> Option<f64> {
    let number = value
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit() && *ch != '-')
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-' || *ch == '.')
        .collect::<String>();

    number.parse().ok()
}

fn stat_value_json(value: f64) -> Value {
    if value.fract() == 0.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn format_filter_value(value: f64) -> String {
    if value.fract() == 0.0 {
        (value as i64).to_string()
    } else {
        value.to_string()
    }
}

fn is_elemental_resistance_modifier(text: &str) -> bool {
    (text.contains("to Fire Resistance")
        || text.contains("to Cold Resistance")
        || text.contains("to Lightning Resistance"))
        && !text.contains("Chaos Resistance")
}

fn normalized_modifier_text(text: &str) -> String {
    text.replace('[', "").replace(']', "").replace('|', " ")
}

fn category_for_item_class(item_class: &str) -> Option<&'static str> {
    let normalized = item_class.to_ascii_lowercase();
    match normalized.as_str() {
        "boots" => Some("armour.boots"),
        "body armours" => Some("armour.chest"),
        "gloves" => Some("armour.gloves"),
        "helmets" => Some("armour.helmet"),
        "shields" => Some("armour.shield"),
        "amulets" => Some("accessory.amulet"),
        "rings" => Some("accessory.ring"),
        "belts" => Some("accessory.belt"),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct TradeSearchApiResponse {
    id: String,
    result: Vec<String>,
    total: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TradeFetchApiResponse {
    result: Vec<FetchResult>,
}

#[derive(Debug, Deserialize)]
struct FetchResult {
    id: String,
    listing: FetchListing,
    item: FetchItem,
}

#[derive(Debug, Deserialize)]
struct FetchListing {
    indexed: Option<String>,
    price: Option<FetchPrice>,
    account: Option<FetchAccount>,
}

#[derive(Debug, Deserialize)]
struct FetchAccount {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FetchPrice {
    #[serde(rename = "type")]
    price_type: Option<String>,
    amount: Option<f64>,
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FetchItem {
    icon: Option<String>,
    name: Option<String>,
    #[serde(rename = "typeLine")]
    type_line: Option<String>,
    #[serde(rename = "baseType")]
    base_type: Option<String>,
    rarity: Option<String>,
    #[serde(rename = "ilvl")]
    item_level: Option<u32>,
    #[serde(rename = "explicitMods")]
    explicit_mods: Option<Vec<FetchMod>>,
    #[serde(rename = "pseudoMods")]
    pseudo_mods: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FetchMod {
    Text(String),
    Object { description: Option<String> },
}

impl FetchMod {
    fn into_text(self) -> Option<String> {
        match self {
            FetchMod::Text(text) => Some(text),
            FetchMod::Object { description } => description,
        }
        .filter(|text| !text.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_item_text;
    use crate::trade::{
        build_fetch_url, build_trade_query, is_blocked_or_rate_limited, map_fetch_response,
    };

    const RARE_BOOTS: &str = "Item Class: Boots
Rarity: Rare
Cataclysm Road
Bound Sandals
--------
Quality: +20%
Energy Shield: 335
--------
Requirements:
Level: 65
Int: 86
--------
Sockets: S
--------
Item Level: 82
--------
+43 to maximum Energy Shield
124% increased Energy Shield
18% increased Rarity of Items found
+31% to Lightning Resistance
+101 to Stun Threshold";

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

    const UNIQUE_HELMET: &str = "Item Class: Helmets
Rarity: Unique
Crown of the Pale King
Cultist Crown
--------
Quality: +20%
--------
Item Level: 84
--------
Allies in your Presence have 10% increased Attack Speed (rune)
{ Unique Modifier }
50(50-75)% increased Spirit
{ Unique Modifier — Life }
Allies in your Presence Regenerate 94.2(50-100) Life per second
{ Unique Modifier — Attribute }
+6(6-12) to all Attributes
{ Unique Modifier }
Companions deal 97(50-100)% increased damage to your Marked targets
{ Unique Modifier }
You can have any number of Companions of different types — Unscalable Value
Darkness howls through ancient bones, a wistful cry";

    #[test]
    fn query_builder_includes_only_selected_supported_filters() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("item should parse");
        let query = build_trade_query(
            &item,
            &[
                "identity:type".to_string(),
                "misc:item_level".to_string(),
                "explicit:0".to_string(),
            ],
        )
        .expect("query should build");

        assert_eq!(query["query"]["type"], "Expert Hexer's Robe");
        assert_eq!(query["query"]["filters"]["misc_filters"]["filters"]["ilvl"]["min"], 72);
        assert!(query["query"]["stats"].as_array().expect("stats array").is_empty());
        assert!(query["query"]["filters"]["trade_filters"].is_object());
    }

    #[test]
    fn query_builder_matches_har_shape_for_mapped_boot_filters() {
        let item = parse_item_text(RARE_BOOTS).expect("boots should parse");
        let query = build_trade_query(
            &item,
            &[
                "category:armour.boots".to_string(),
                "stat:explicit.stat_4052037485:0".to_string(),
                "stat:explicit.stat_4015621042:1".to_string(),
                "stat:explicit.stat_3917489142:2".to_string(),
                "stat:pseudo.pseudo_total_elemental_resistance".to_string(),
                "stat:explicit.stat_915769802:4".to_string(),
            ],
        )
        .expect("query should build");

        assert_eq!(query["query"]["status"]["option"], "any");
        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"],
            "armour.boots"
        );
        assert_eq!(query["query"]["sort"], serde_json::Value::Null);
        assert_eq!(query["sort"]["price"], "asc");

        let filters = query["query"]["stats"][0]["filters"]
            .as_array()
            .expect("stat filters");
        let observed = filters
            .iter()
            .map(|filter| {
                (
                    filter["id"].as_str().expect("id"),
                    filter["value"]["min"].as_i64().expect("min"),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![
                ("explicit.stat_4052037485", 43),
                ("explicit.stat_4015621042", 124),
                ("explicit.stat_3917489142", 18),
                ("pseudo.pseudo_total_elemental_resistance", 31),
                ("explicit.stat_915769802", 101),
            ]
        );
    }

    #[test]
    fn query_builder_uses_unique_base_type_not_unique_name() {
        let item = parse_item_text(UNIQUE_HELMET).expect("unique helmet should parse");
        let query = build_trade_query(&item, &["identity:type".to_string()]).expect("query should build");

        assert_eq!(query["query"]["type"], "Cultist Crown");
    }

    #[test]
    fn query_builder_maps_unique_ranged_and_rune_stats() {
        let item = parse_item_text(UNIQUE_HELMET).expect("unique helmet should parse");
        let query = build_trade_query(
            &item,
            &[
                "category:armour.helmet".to_string(),
                "stat:rune.stat_1998951374:0".to_string(),
                "stat:explicit.stat_3984865854:2".to_string(),
                "stat:explicit.stat_4010677958:4".to_string(),
                "stat:explicit.stat_1379411836:6".to_string(),
                "stat:explicit.stat_1067622524:8".to_string(),
            ],
        )
        .expect("query should build");

        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"],
            "armour.helmet"
        );

        let filters = query["query"]["stats"][0]["filters"]
            .as_array()
            .expect("stat filters");
        let observed = filters
            .iter()
            .map(|filter| {
                (
                    filter["id"].as_str().expect("id"),
                    filter["value"]["min"].as_f64().expect("min"),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![
                ("rune.stat_1998951374", 10.0),
                ("explicit.stat_3984865854", 50.0),
                ("explicit.stat_4010677958", 94.2),
                ("explicit.stat_1379411836", 6.0),
                ("explicit.stat_1067622524", 97.0),
            ]
        );
    }

    #[test]
    fn fetch_url_includes_result_ids_query_realm_and_pseudos() {
        let ids = (0..10).map(|index| format!("id{index}")).collect::<Vec<_>>();
        let url = build_fetch_url(
            "d8LMyZrRsJ",
            &ids,
            &["pseudo.pseudo_total_elemental_resistance".to_string()],
        )
        .expect("fetch url should build");

        assert_eq!(
            url,
            "https://www.pathofexile.com/api/trade2/fetch/id0,id1,id2,id3,id4,id5,id6,id7,id8,id9?query=d8LMyZrRsJ&realm=poe2&pseudos[]=pseudo.pseudo_total_elemental_resistance"
        );
    }

    #[test]
    fn fetch_response_mapper_extracts_listing_summary_without_hideout_token() {
        let response = r#"{
            "result": [{
                "id": "87dc03118c0a90f95957ae9b5495f322d2de521879fb97d093ad6a71dafcde68",
                "listing": {
                    "indexed": "2026-07-04T14:26:51Z",
                    "price": { "type": "~b/o", "amount": 5, "currency": "chaos" },
                    "account": { "name": "SGM#6552", "online": null },
                    "hideout_token": "secret-token"
                },
                "item": {
                    "icon": "https://web.poecdn.com/image.png",
                    "name": "Cataclysm Road",
                    "typeLine": "Bound Sandals",
                    "baseType": "Bound Sandals",
                    "rarity": "Rare",
                    "ilvl": 82,
                    "explicitMods": [
                        { "description": "124% increased [EnergyShield|Energy Shield]" },
                        { "description": "+371 to [StunThreshold|Stun Threshold]" }
                    ],
                    "pseudoMods": ["+45% total Elemental Resistance"]
                }
            }]
        }"#;

        let listings = map_fetch_response(response).expect("fetch response should map");

        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0].id, "87dc03118c0a90f95957ae9b5495f322d2de521879fb97d093ad6a71dafcde68");
        assert_eq!(listings[0].price.as_ref().expect("price").amount, 5.0);
        assert_eq!(listings[0].price.as_ref().expect("price").currency, "chaos");
        assert_eq!(listings[0].account_name.as_deref(), Some("SGM#6552"));
        assert_eq!(listings[0].item.name.as_deref(), Some("Cataclysm Road"));
        assert_eq!(listings[0].item.item_level, Some(82));
        assert_eq!(listings[0].item.explicit_mods.len(), 2);
        assert_eq!(listings[0].item.pseudo_mods, vec!["+45% total Elemental Resistance"]);

        let serialized = serde_json::to_string(&listings[0]).expect("listing should serialize");
        assert!(!serialized.contains("hideout_token"));
        assert!(!serialized.contains("secret-token"));
    }

    #[test]
    fn api_block_and_rate_limit_statuses_are_detected() {
        assert!(is_blocked_or_rate_limited(403));
        assert!(is_blocked_or_rate_limited(429));
        assert!(!is_blocked_or_rate_limited(500));
        assert!(!is_blocked_or_rate_limited(200));
    }
}
