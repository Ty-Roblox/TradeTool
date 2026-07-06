use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::models::{
    AppDiagnostic, CapturedItem, FilterValueOverride, ItemModifier, TradeListing, TradeListingItem,
    TradePrice, TradeSearchResponse, TradeTextSegment,
};
use crate::poe_keywords::describe_keyword;
use crate::stat_patterns::STAT_PATTERNS;
use serde::Deserialize;
use serde_json::{json, Value};

const TRADE_BASE_URL: &str = "https://www.pathofexile.com";
const FETCH_PAGE_SIZE: usize = 10;
const QUICK_JEWEL_FILTERS_JSON: &str = include_str!("../../src/lib/quick-jewel-filters.json");
const EXACT_SELECTED_EXPLICIT_AFFIXES_FILTER_ID: &str = "misc:exact_selected_explicit_affixes";
const EXACT_SELECTED_PREFIX_AFFIXES_FILTER_ID: &str = "misc:exact_selected_prefix_affixes";
const EXACT_SELECTED_SUFFIX_AFFIXES_FILTER_ID: &str = "misc:exact_selected_suffix_affixes";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuickJewelFilter {
    id: String,
    label: String,
    base_type: String,
    stats: Vec<QuickJewelStat>,
}

#[derive(Debug, Deserialize)]
struct QuickJewelStat {
    id: String,
    label: String,
    min: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeFilterSpec {
    pub id: String,
    pub label: String,
    pub selected_by_default: bool,
    pub source_modifier_index: Option<usize>,
    pub source: Option<String>,
    pub affix_side: Option<String>,
    pub score: Option<u8>,
    pub selection_reason: Option<String>,
    pub profile_ids: Vec<String>,
    kind: TradeFilterKind,
}

impl TradeFilterSpec {
    pub fn default_min(&self) -> Option<f64> {
        match &self.kind {
            TradeFilterKind::Stat { value, .. } => *value,
            TradeFilterKind::Category(_)
            | TradeFilterKind::ItemType { .. }
            | TradeFilterKind::ExactSelectedAffixes { .. } => None,
        }
    }

    pub fn default_max(&self) -> Option<f64> {
        match &self.kind {
            TradeFilterKind::Stat { max_value, .. } => *max_value,
            TradeFilterKind::Category(_)
            | TradeFilterKind::ItemType { .. }
            | TradeFilterKind::ExactSelectedAffixes { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExactAffixScope {
    Explicit,
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, PartialEq)]
enum TradeFilterKind {
    Category(String),
    ItemType {
        type_name: String,
        category: Option<String>,
    },
    Stat {
        stat_id: String,
        value: Option<f64>,
        max_value: Option<f64>,
    },
    ExactSelectedAffixes {
        scope: ExactAffixScope,
    },
}

pub fn trade_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let mut specs = Vec::new();

    if let Some(category) = item.item_class.as_deref().and_then(category_for_item_class) {
        specs.push(TradeFilterSpec {
            id: format!("category:{category}"),
            label: format!("Category: {}", item.item_class.as_deref().unwrap_or("Item")),
            selected_by_default: true,
            source_modifier_index: None,
            source: None,
            affix_side: None,
            score: None,
            selection_reason: Some(
                "Category keeps the trade search on this item class.".to_string(),
            ),
            profile_ids: category_profile_ids(item),
            kind: TradeFilterKind::Category(category.to_string()),
        });
    }

    specs.extend(empty_affix_filter_specs(item));

    let stat_specs = stat_filter_specs(item);
    if stat_specs.iter().any(is_explicit_stat_spec) {
        specs.push(exact_selected_affix_filter_spec(
            ExactAffixScope::Explicit,
            item.rarity.as_deref() == Some("Magic"),
        ));
    }
    if stat_specs
        .iter()
        .any(|spec| is_explicit_stat_spec(spec) && spec.affix_side.as_deref() == Some("prefix"))
    {
        specs.push(exact_selected_affix_filter_spec(
            ExactAffixScope::Prefix,
            false,
        ));
    }
    if stat_specs
        .iter()
        .any(|spec| is_explicit_stat_spec(spec) && spec.affix_side.as_deref() == Some("suffix"))
    {
        specs.push(exact_selected_affix_filter_spec(
            ExactAffixScope::Suffix,
            false,
        ));
    }

    specs.extend(stat_specs);
    specs
}

fn exact_selected_affix_filter_spec(
    scope: ExactAffixScope,
    quick_profile: bool,
) -> TradeFilterSpec {
    let (id, label, affix_side, reason) = match scope {
        ExactAffixScope::Explicit => (
            EXACT_SELECTED_EXPLICIT_AFFIXES_FILTER_ID,
            "Only selected explicit affixes",
            None,
            "Adds a # Modifiers exact-count trade filter and hides fetched listings with extra explicit mods.",
        ),
        ExactAffixScope::Prefix => (
            EXACT_SELECTED_PREFIX_AFFIXES_FILTER_ID,
            "Only selected prefixes",
            Some("prefix"),
            "Requires the result to have exactly the selected explicit prefix count.",
        ),
        ExactAffixScope::Suffix => (
            EXACT_SELECTED_SUFFIX_AFFIXES_FILTER_ID,
            "Only selected suffixes",
            Some("suffix"),
            "Requires the result to have exactly the selected explicit suffix count.",
        ),
    };

    let mut profile_ids = profile_ids(&["exact"]);
    if quick_profile {
        profile_ids.push("quick".to_string());
    }

    TradeFilterSpec {
        id: id.to_string(),
        label: label.to_string(),
        selected_by_default: quick_profile,
        source_modifier_index: None,
        source: Some("pseudo".to_string()),
        affix_side: affix_side.map(ToOwned::to_owned),
        score: Some(7),
        selection_reason: Some(reason.to_string()),
        profile_ids,
        kind: TradeFilterKind::ExactSelectedAffixes { scope },
    }
}

fn empty_affix_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let Some((max_prefixes, max_suffixes)) = max_affix_slots(item) else {
        return Vec::new();
    };

    let prefix_count = item
        .explicit_mods
        .iter()
        .filter(|modifier| modifier.affix_side.as_deref() == Some("prefix"))
        .count();
    let suffix_count = item
        .explicit_mods
        .iter()
        .filter(|modifier| modifier.affix_side.as_deref() == Some("suffix"))
        .count();

    if prefix_count == 0 && suffix_count == 0 {
        return Vec::new();
    }

    let empty_prefixes = max_prefixes.saturating_sub(prefix_count);
    let empty_suffixes = max_suffixes.saturating_sub(suffix_count);
    let empty_affixes = empty_prefixes + empty_suffixes;
    let mut specs = Vec::new();

    if empty_prefixes > 0 {
        specs.push(empty_affix_filter_spec(
            "stat:pseudo.pseudo_number_of_empty_prefix_mods",
            "pseudo.pseudo_number_of_empty_prefix_mods",
            format!("Empty Prefix Modifiers: {empty_prefixes}+"),
            empty_prefixes,
            "Open prefixes matter when pricing crafting bases.",
        ));
    }

    if empty_suffixes > 0 {
        specs.push(empty_affix_filter_spec(
            "stat:pseudo.pseudo_number_of_empty_suffix_mods",
            "pseudo.pseudo_number_of_empty_suffix_mods",
            format!("Empty Suffix Modifiers: {empty_suffixes}+"),
            empty_suffixes,
            "Open suffixes matter when pricing crafting bases.",
        ));
    }

    if empty_affixes > 0 {
        specs.push(empty_affix_filter_spec(
            "stat:pseudo.pseudo_number_of_empty_affix_mods",
            "pseudo.pseudo_number_of_empty_affix_mods",
            format!("Empty Modifiers: {empty_affixes}+"),
            empty_affixes,
            "Total open affixes are useful for broad crafting-base searches.",
        ));
    }

    specs
}

fn max_affix_slots(item: &CapturedItem) -> Option<(usize, usize)> {
    match item.rarity.as_deref() {
        Some("Rare") => Some((3, 3)),
        Some("Magic") => Some((1, 1)),
        _ => None,
    }
}

fn empty_affix_filter_spec(
    id: &str,
    stat_id: &str,
    label: String,
    value: usize,
    reason: &str,
) -> TradeFilterSpec {
    TradeFilterSpec {
        id: id.to_string(),
        label,
        selected_by_default: false,
        source_modifier_index: None,
        source: Some("pseudo".to_string()),
        affix_side: empty_affix_side(stat_id).map(ToOwned::to_owned),
        score: Some(6),
        selection_reason: Some(reason.to_string()),
        profile_ids: profile_ids(&["crafting-base"]),
        kind: TradeFilterKind::Stat {
            stat_id: stat_id.to_string(),
            value: Some(value as f64),
            max_value: None,
        },
    }
}

fn empty_affix_side(stat_id: &str) -> Option<&'static str> {
    if stat_id.contains("empty_prefix") {
        Some("prefix")
    } else if stat_id.contains("empty_suffix") {
        Some("suffix")
    } else {
        None
    }
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

#[allow(dead_code)]
pub fn build_trade_query(
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> Result<Value, String> {
    build_trade_query_with_values(item, selected_filter_ids, &[])
}

pub fn build_trade_query_with_values(
    item: &CapturedItem,
    selected_filter_ids: &[String],
    selected_filter_values: &[FilterValueOverride],
) -> Result<Value, String> {
    validate_selected_filter_ids(item, selected_filter_ids)?;

    let selected = selected_filter_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let value_overrides = validated_value_overrides(&selected, selected_filter_values)?;
    let all_specs = all_trade_filter_specs(item);
    let selected_specs = all_specs
        .iter()
        .filter(|spec| selected.contains(spec.id.as_str()))
        .collect::<Vec<_>>();
    let exact_affix_counts = selected_exact_affix_counts(&all_specs, &selected)?;
    let mut stat_filters = Vec::new();
    for spec in &selected_specs {
        match &spec.kind {
            TradeFilterKind::Stat {
                stat_id,
                value,
                max_value,
            } => {
                let (value, max_value) =
                    filter_value_range(spec.id.as_str(), *value, *max_value, &value_overrides)?;
                let mut filter = json!({
                    "id": stat_id,
                    "disabled": false
                });

                if let Some(value) = value {
                    filter["value"]["min"] = stat_value_json(value);
                }
                if let Some(max_value) = max_value {
                    filter["value"]["max"] = stat_value_json(max_value);
                }

                stat_filters.push(filter);
            }
            TradeFilterKind::Category(_)
            | TradeFilterKind::ItemType { .. }
            | TradeFilterKind::ExactSelectedAffixes { .. } => {}
        }
    }
    for exact_count in &exact_affix_counts {
        stat_filters.push(json!({
            "id": exact_affix_count_stat_id(exact_count.scope),
            "disabled": false,
            "value": {
                "min": exact_count.count,
                "max": exact_count.count
            }
        }));
    }

    let mut query = json!({
        "query": {
            "status": { "option": "securable" },
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
            let (min, max) = filter_value_range(
                "misc:item_level",
                Some(item_level as f64),
                None,
                &value_overrides,
            )?;
            if let Some(min) = min {
                query["query"]["filters"]["misc_filters"]["filters"]["ilvl"]["min"] =
                    stat_value_json(min);
            }
            if let Some(max) = max {
                query["query"]["filters"]["misc_filters"]["filters"]["ilvl"]["max"] =
                    stat_value_json(max);
            }
        }
    }

    if selected.contains("property:quality") {
        if let Some(quality) = item.quality {
            let (min, max) = filter_value_range(
                "property:quality",
                Some(quality as f64),
                None,
                &value_overrides,
            )?;
            if let Some(min) = min {
                query["query"]["filters"]["misc_filters"]["filters"]["quality"]["min"] =
                    stat_value_json(min);
            }
            if let Some(max) = max {
                query["query"]["filters"]["misc_filters"]["filters"]["quality"]["max"] =
                    stat_value_json(max);
            }
        }
    }

    if selected.contains("property:gem_level") {
        if let Some(level) = gem_level(item) {
            let (min, max) = filter_value_range(
                "property:gem_level",
                Some(level as f64),
                None,
                &value_overrides,
            )?;
            if let Some(min) = min {
                query["query"]["filters"]["misc_filters"]["filters"]["gem_level"]["min"] =
                    stat_value_json(min);
            }
            if let Some(max) = max {
                query["query"]["filters"]["misc_filters"]["filters"]["gem_level"]["max"] =
                    stat_value_json(max);
            }
        }
    }

    if selected.contains("property:sockets") {
        if let Some(count) = item.sockets.as_deref().and_then(socket_count) {
            let (min, max) = filter_value_range(
                "property:sockets",
                Some(count as f64),
                None,
                &value_overrides,
            )?;
            if let Some(min) = min {
                query["query"]["filters"]["equipment_filters"]["filters"]["rune_sockets"]["min"] =
                    stat_value_json(min);
            }
            if let Some(max) = max {
                query["query"]["filters"]["equipment_filters"]["filters"]["rune_sockets"]["max"] =
                    stat_value_json(max);
            }
            query["query"]["filters"]["equipment_filters"]["disabled"] = json!(false);
        }
    }

    for spec in &selected_specs {
        match &spec.kind {
            TradeFilterKind::Category(category) => {
                query["query"]["filters"]["type_filters"]["filters"]["category"]["option"] =
                    json!(category);
                query["query"]["filters"]["type_filters"]["disabled"] = json!(false);
            }
            TradeFilterKind::ItemType {
                type_name,
                category,
            } => {
                query["query"]["type"] = json!(type_name);
                if let Some(category) = category {
                    query["query"]["filters"]["type_filters"]["filters"]["category"]["option"] =
                        json!(category);
                    query["query"]["filters"]["type_filters"]["disabled"] = json!(false);
                }
            }
            TradeFilterKind::Stat { .. } | TradeFilterKind::ExactSelectedAffixes { .. } => {}
        }
    }

    Ok(query)
}

fn validated_value_overrides<'a>(
    selected_filter_ids: &HashSet<&str>,
    selected_filter_values: &'a [FilterValueOverride],
) -> Result<HashMap<&'a str, &'a FilterValueOverride>, String> {
    let mut overrides = HashMap::new();

    for override_value in selected_filter_values {
        if !selected_filter_ids.contains(override_value.id.as_str()) {
            continue;
        }

        if let (Some(min), Some(max)) = (override_value.min, override_value.max) {
            if min > max {
                return Err(format!(
                    "Filter {} has a minimum greater than its maximum.",
                    override_value.id
                ));
            }
        }

        overrides.insert(override_value.id.as_str(), override_value);
    }

    Ok(overrides)
}

fn filter_value_range(
    filter_id: &str,
    default_min: Option<f64>,
    default_max: Option<f64>,
    overrides: &HashMap<&str, &FilterValueOverride>,
) -> Result<(Option<f64>, Option<f64>), String> {
    let Some(override_value) = overrides.get(filter_id) else {
        return Ok((default_min, default_max));
    };

    if let (Some(min), Some(max)) = (override_value.min, override_value.max) {
        if min > max {
            return Err(format!(
                "Filter {filter_id} has a minimum greater than its maximum."
            ));
        }
    }

    Ok((
        override_value.min.or(default_min),
        override_value.max.or(default_max),
    ))
}

fn validate_selected_filter_ids(
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> Result<(), String> {
    let valid_ids = supported_filter_ids(item);
    let mut invalid_ids = selected_filter_ids
        .iter()
        .filter(|id| !valid_ids.contains(id.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    invalid_ids.sort();
    invalid_ids.dedup();

    if invalid_ids.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Unknown selected filter IDs: {}. Re-parse the current item and try again.",
            invalid_ids.join(", ")
        ))
    }
}

fn supported_filter_ids(item: &CapturedItem) -> HashSet<String> {
    let mut ids = all_trade_filter_specs(item)
        .into_iter()
        .map(|spec| spec.id)
        .collect::<HashSet<_>>();

    if item.base_type.is_some() {
        ids.insert("identity:type".to_string());
    }
    if item.rarity.is_some() {
        ids.insert("identity:rarity".to_string());
    }
    if item.item_level.is_some() {
        ids.insert("misc:item_level".to_string());
    }
    if item.quality.is_some() {
        ids.insert("property:quality".to_string());
    }
    if gem_level(item).is_some() {
        ids.insert("property:gem_level".to_string());
    }
    if item.sockets.as_deref().and_then(socket_count).is_some() {
        ids.insert("property:sockets".to_string());
    }

    ids
}

pub fn selected_pseudo_stat_ids(
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> Vec<String> {
    let selected = selected_filter_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    all_trade_filter_specs(item)
        .into_iter()
        .filter(|spec| selected.contains(spec.id.as_str()))
        .filter_map(|spec| match spec.kind {
            TradeFilterKind::Stat { stat_id, .. } if stat_id.starts_with("pseudo.") => {
                Some(stat_id)
            }
            TradeFilterKind::ExactSelectedAffixes { scope } => {
                Some(exact_affix_count_stat_id(scope).to_string())
            }
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExactAffixCount {
    scope: ExactAffixScope,
    count: usize,
}

fn selected_exact_affix_counts(
    all_specs: &[TradeFilterSpec],
    selected_filter_ids: &HashSet<&str>,
) -> Result<Vec<ExactAffixCount>, String> {
    let mut counts = Vec::new();

    for scope in [
        ExactAffixScope::Explicit,
        ExactAffixScope::Prefix,
        ExactAffixScope::Suffix,
    ] {
        if !selected_filter_ids.contains(exact_affix_filter_id(scope)) {
            continue;
        }

        let count = all_specs
            .iter()
            .filter(|spec| selected_filter_ids.contains(spec.id.as_str()))
            .filter(|spec| exact_affix_scope_matches(scope, spec))
            .count();

        if count == 0 {
            return Err(format!(
                "Select at least one explicit {}modifier filter before using {}.",
                exact_affix_error_scope(scope),
                exact_affix_label(scope)
            ));
        }

        counts.push(ExactAffixCount { scope, count });
    }

    Ok(counts)
}

fn selected_exact_explicit_affix_count(
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> Result<Option<usize>, String> {
    let selected = selected_filter_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    selected_exact_affix_counts(&all_trade_filter_specs(item), &selected).map(|counts| {
        counts
            .into_iter()
            .find(|count| count.scope == ExactAffixScope::Explicit)
            .map(|count| count.count)
    })
}

fn apply_exact_selected_explicit_affix_filter(
    listings: Vec<TradeListing>,
    exact_count: Option<usize>,
) -> Vec<TradeListing> {
    let Some(exact_count) = exact_count else {
        return listings;
    };

    listings
        .into_iter()
        .filter(|listing| listing.item.explicit_mods.len() == exact_count)
        .collect()
}

fn exact_affix_scope_matches(scope: ExactAffixScope, spec: &TradeFilterSpec) -> bool {
    if !is_explicit_stat_spec(spec) {
        return false;
    }

    match scope {
        ExactAffixScope::Explicit => true,
        ExactAffixScope::Prefix => spec.affix_side.as_deref() == Some("prefix"),
        ExactAffixScope::Suffix => spec.affix_side.as_deref() == Some("suffix"),
    }
}

fn exact_affix_filter_id(scope: ExactAffixScope) -> &'static str {
    match scope {
        ExactAffixScope::Explicit => EXACT_SELECTED_EXPLICIT_AFFIXES_FILTER_ID,
        ExactAffixScope::Prefix => EXACT_SELECTED_PREFIX_AFFIXES_FILTER_ID,
        ExactAffixScope::Suffix => EXACT_SELECTED_SUFFIX_AFFIXES_FILTER_ID,
    }
}

fn exact_affix_count_stat_id(scope: ExactAffixScope) -> &'static str {
    match scope {
        ExactAffixScope::Explicit => "pseudo.pseudo_number_of_affix_mods",
        ExactAffixScope::Prefix => "pseudo.pseudo_number_of_prefix_mods",
        ExactAffixScope::Suffix => "pseudo.pseudo_number_of_suffix_mods",
    }
}

fn exact_affix_label(scope: ExactAffixScope) -> &'static str {
    match scope {
        ExactAffixScope::Explicit => "Only selected explicit affixes",
        ExactAffixScope::Prefix => "Only selected prefixes",
        ExactAffixScope::Suffix => "Only selected suffixes",
    }
}

fn exact_affix_error_scope(scope: ExactAffixScope) -> &'static str {
    match scope {
        ExactAffixScope::Explicit => "",
        ExactAffixScope::Prefix => "prefix ",
        ExactAffixScope::Suffix => "suffix ",
    }
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
        .map(|result| {
            let hideout_token = result
                .listing
                .hideout_token
                .filter(|token| !token.trim().is_empty());
            let price = result.listing.price.and_then(|price| {
                Some(TradePrice {
                    price_type: price.price_type,
                    amount: price.amount?,
                    currency: price.currency?,
                })
            });
            let is_exact_buyout = price.as_ref().is_some_and(is_exact_buyout_price);
            let hideout_token = is_exact_buyout.then_some(hideout_token).flatten();

            let explicit_mods_raw = result
                .item
                .explicit_mods
                .unwrap_or_default()
                .into_iter()
                .filter_map(FetchMod::into_text)
                .collect::<Vec<_>>();
            let pseudo_mods_raw = result.item.pseudo_mods.unwrap_or_default();

            TradeListing {
                id: result.id,
                indexed: result.listing.indexed,
                price,
                account_name: result.listing.account.and_then(|account| account.name),
                can_teleport: is_exact_buyout,
                hideout_token,
                item: TradeListingItem {
                    icon: result.item.icon,
                    name: result.item.name.filter(|name| !name.trim().is_empty()),
                    type_line: result.item.type_line,
                    base_type: result.item.base_type,
                    rarity: result.item.rarity,
                    item_level: result.item.item_level,
                    explicit_mods: explicit_mods_raw
                        .iter()
                        .map(|modifier| expand_trade_text_tags(modifier))
                        .collect(),
                    pseudo_mods: pseudo_mods_raw
                        .iter()
                        .map(|modifier| expand_trade_text_tags(modifier))
                        .collect(),
                    explicit_mod_segments: explicit_mods_raw
                        .iter()
                        .map(|modifier| trade_text_segments(modifier))
                        .collect(),
                    pseudo_mod_segments: pseudo_mods_raw
                        .iter()
                        .map(|modifier| trade_text_segments(modifier))
                        .collect(),
                },
            }
        })
        .collect())
}

fn is_exact_buyout_price(price: &TradePrice) -> bool {
    price.price_type.as_deref() == Some("~b/o")
}

pub async fn search_trade(
    league: &str,
    item: &CapturedItem,
    selected_filter_ids: &[String],
    selected_filter_values: &[FilterValueOverride],
) -> Result<TradeSearchResponse, String> {
    let league = sanitize_league(league)?;
    let query = build_trade_query_with_values(item, selected_filter_ids, selected_filter_values)?;
    let exact_explicit_affix_count =
        selected_exact_explicit_affix_count(item, selected_filter_ids)?;
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
        return Err(format_trade_api_error(
            &status.to_string(),
            &body,
            item,
            selected_filter_ids,
        ));
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
    let mut diagnostics = Vec::new();

    if first_page_ids.is_empty() {
        warning = Some("The POE2 trade API returned no matching listings.".to_string());
    } else {
        let url = build_fetch_url(&body.id, &first_page_ids, &pseudo_stat_ids)?;
        fetch_url = Some(url.clone());

        match fetch_trade_listings(&url).await {
            Ok(fetched) => {
                let fetched_len = fetched.len();
                listings = fetched
                    .into_iter()
                    .filter(|listing| listing.can_teleport)
                    .collect();
                let exact_buyout_len = listings.len();
                listings = apply_exact_selected_explicit_affix_filter(
                    listings,
                    exact_explicit_affix_count,
                );

                if fetched_len > 0 && listings.is_empty() {
                    warning = Some(
                        if exact_explicit_affix_count.is_some() && exact_buyout_len > 0 {
                            "The first fetched page had exact-buyout listings, but none had only the selected explicit affixes.".to_string()
                        } else {
                            "The first fetched page had no exact-buyout listings available for TP."
                                .to_string()
                        },
                    );
                }
            }
            Err(error) => {
                diagnostics.push(AppDiagnostic {
                    code: "listing_fetch_failed".to_string(),
                    message: "First-page listing fetch failed.".to_string(),
                    detail: Some(error.clone()),
                });
                warning = Some(error);
            }
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
        diagnostics,
    })
}

fn format_trade_api_error(
    status: &str,
    body: &str,
    item: &CapturedItem,
    selected_filter_ids: &[String],
) -> String {
    let api_message = parse_api_error_message(body)
        .unwrap_or_else(|| body.trim().to_string())
        .trim()
        .to_string();
    let selected_ids = format_id_list(selected_filter_ids);
    let stat_ids = format_id_list(&selected_stat_ids(item, selected_filter_ids));

    format!(
        "POE2 trade API returned {status}: {api_message}. Selected filter IDs: {selected_ids}. Sent stat IDs: {stat_ids}."
    )
}

fn parse_api_error_message(body: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct ApiErrorEnvelope {
        error: Option<ApiErrorBody>,
    }

    #[derive(Debug, Deserialize)]
    struct ApiErrorBody {
        code: Option<i64>,
        message: Option<String>,
    }

    let parsed = serde_json::from_str::<ApiErrorEnvelope>(body).ok()?;
    let error = parsed.error?;

    match (error.code, error.message) {
        (Some(code), Some(message)) => Some(format!("{message} (code {code})")),
        (None, Some(message)) => Some(message),
        (Some(code), None) => Some(format!("API error code {code}")),
        (None, None) => None,
    }
}

fn selected_stat_ids(item: &CapturedItem, selected_filter_ids: &[String]) -> Vec<String> {
    let selected = selected_filter_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut stat_ids = all_trade_filter_specs(item)
        .into_iter()
        .filter(|spec| selected.contains(spec.id.as_str()))
        .filter_map(|spec| match spec.kind {
            TradeFilterKind::Stat { stat_id, .. } => Some(stat_id),
            TradeFilterKind::ExactSelectedAffixes { scope } => {
                Some(exact_affix_count_stat_id(scope).to_string())
            }
            TradeFilterKind::Category(_) | TradeFilterKind::ItemType { .. } => None,
        })
        .collect::<Vec<_>>();

    stat_ids.sort();
    stat_ids.dedup();
    stat_ids
}

fn format_id_list(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
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
        return Err(format!(
            "POE2 trade listing fetch returned {status}: {}",
            body.trim()
        ));
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

fn all_trade_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let mut specs = trade_filter_specs(item);
    specs.extend(quick_filter_specs());
    specs
}

fn quick_filter_specs() -> Vec<TradeFilterSpec> {
    let mut specs = Vec::new();

    for jewel in quick_jewel_filters() {
        specs.push(TradeFilterSpec {
            id: format!("quick:jewel:{}:base", jewel.id),
            label: format!("Jewel: {}", jewel.label),
            selected_by_default: false,
            source_modifier_index: None,
            source: None,
            affix_side: None,
            score: None,
            selection_reason: None,
            profile_ids: Vec::new(),
            kind: TradeFilterKind::ItemType {
                type_name: jewel.base_type.clone(),
                category: Some("jewel".to_string()),
            },
        });

        for stat in &jewel.stats {
            specs.push(TradeFilterSpec {
                id: format!("quick:jewel:{}:stat:{}", jewel.id, stat.id),
                label: format!("{}: {}", jewel.label, stat.label),
                selected_by_default: false,
                source_modifier_index: None,
                source: None,
                affix_side: None,
                score: None,
                selection_reason: None,
                profile_ids: Vec::new(),
                kind: TradeFilterKind::Stat {
                    stat_id: stat.id.clone(),
                    value: stat.min,
                    max_value: None,
                },
            });
        }
    }

    specs
}

fn quick_jewel_filters() -> &'static [QuickJewelFilter] {
    static FILTERS: OnceLock<Vec<QuickJewelFilter>> = OnceLock::new();

    FILTERS
        .get_or_init(|| {
            serde_json::from_str(QUICK_JEWEL_FILTERS_JSON)
                .expect("quick jewel filter catalog should be valid JSON")
        })
        .as_slice()
}

fn stat_filter_specs(item: &CapturedItem) -> Vec<TradeFilterSpec> {
    let mut keyed_specs = Vec::new();
    let mut elemental_resistance_total = 0.0;
    let mut first_resistance_index = None;

    for modifier in &item.explicit_mods {
        let text = normalized_modifier_text(&modifier.text);

        if let Some((stat_id, label, value)) = mapped_charm_slots(&text) {
            keyed_specs.push((
                modifier.index,
                stat_filter_spec(stat_id, label, modifier, Some(value), None),
            ));
        } else if let Some((stat_id, label, max_value)) = mapped_reduced_poison_duration(&text) {
            keyed_specs.push((
                modifier.index,
                stat_filter_spec(stat_id, label, modifier, None, Some(max_value)),
            ));
        } else if let Some((stat_id, label, value)) =
            mapped_official_stat(&text, item.item_class.as_deref())
        {
            keyed_specs.push((
                modifier.index,
                stat_filter_spec(stat_id, label, modifier, value, None),
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
            stat_filter_spec_with_source(
                "pseudo.pseudo_total_elemental_resistance".to_string(),
                format!(
                    "Total Elemental Resistance: {}%+",
                    format_filter_value(elemental_resistance_total)
                ),
                "stat:pseudo.pseudo_total_elemental_resistance".to_string(),
                first_resistance_index,
                Some("pseudo".to_string()),
                None,
                Some(elemental_resistance_total),
                None,
            ),
        ));
    }

    apply_quick_profile_selection(&mut keyed_specs);
    keyed_specs.sort_by_key(|(index, _)| *index);
    keyed_specs.into_iter().map(|(_, spec)| spec).collect()
}

fn stat_filter_spec(
    stat_id: String,
    label: String,
    modifier: &ItemModifier,
    value: Option<f64>,
    max_value: Option<f64>,
) -> TradeFilterSpec {
    stat_filter_spec_with_source(
        stat_id.clone(),
        label,
        format!("stat:{stat_id}:{}", modifier.index),
        Some(modifier.index),
        modifier.source.clone(),
        modifier.affix_side.clone(),
        value,
        max_value,
    )
}

fn stat_filter_spec_with_source(
    stat_id: String,
    label: String,
    id: String,
    source_modifier_index: Option<usize>,
    source: Option<String>,
    affix_side: Option<String>,
    value: Option<f64>,
    max_value: Option<f64>,
) -> TradeFilterSpec {
    let (score, selection_reason) = stat_score_and_reason(&stat_id, &label, value, max_value);

    TradeFilterSpec {
        id,
        label,
        selected_by_default: false,
        source_modifier_index,
        source,
        affix_side,
        score: Some(score),
        selection_reason: Some(selection_reason),
        profile_ids: profile_ids(&["exact"]),
        kind: TradeFilterKind::Stat {
            stat_id,
            value,
            max_value,
        },
    }
}

fn apply_quick_profile_selection(keyed_specs: &mut [(usize, TradeFilterSpec)]) {
    let selected_ids = {
        let has_total_elemental_resistance = keyed_specs
            .iter()
            .any(|(_, spec)| is_total_elemental_resistance_spec(spec));
        let mut scored = keyed_specs
            .iter()
            .filter(|(_, spec)| {
                !(has_total_elemental_resistance && is_individual_elemental_resistance_spec(spec))
            })
            .filter_map(|(index, spec)| spec.score.map(|score| (score, *index, spec.id.clone())))
            .filter(|(score, _, _)| *score >= 3)
            .collect::<Vec<_>>();

        scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        scored
            .into_iter()
            .take(4)
            .map(|(_, _, id)| id)
            .collect::<HashSet<_>>()
    };

    for (_, spec) in keyed_specs {
        if selected_ids.contains(&spec.id) {
            spec.selected_by_default = true;
            if !spec.profile_ids.iter().any(|profile| profile == "quick") {
                spec.profile_ids.push("quick".to_string());
            }
        }
    }
}

fn stat_score_and_reason(
    stat_id: &str,
    label: &str,
    value: Option<f64>,
    max_value: Option<f64>,
) -> (u8, String) {
    let label_lower = label.to_ascii_lowercase();

    if stat_id == "pseudo.pseudo_total_elemental_resistance" {
        return scored_reason(
            8,
            pricing_roll_bonus(value, &[60.0, 90.0]),
            "Total elemental resistance is a high-signal pseudo stat.",
        );
    }

    if label_lower.contains("rarity of items") {
        return scored_reason(
            8,
            pricing_roll_bonus(value, &[20.0, 35.0]),
            "Item rarity is a high-signal magic-find price stat.",
        );
    }

    if label_lower.contains("charm slot") {
        return (
            8,
            "Charm slots are scarce and strongly affect belt value.".to_string(),
        );
    }

    if label_lower.contains("level of all") || label_lower.contains("skills") {
        return (8, "Skill levels are usually a premium affix.".to_string());
    }

    if label_lower.contains("maximum life") {
        return scored_reason(
            7,
            pricing_roll_bonus(value, &[90.0, 130.0]),
            "Life is a high-signal price stat.",
        );
    }

    if label_lower.contains("maximum energy shield") {
        return scored_reason(
            7,
            pricing_roll_bonus(value, &[80.0, 140.0]),
            "Flat Energy Shield is a high-signal defensive stat.",
        );
    }

    if label_lower.contains("resistance") {
        return (
            6,
            "Resistance rolls are common trade comparators.".to_string(),
        );
    }

    if label_lower.contains("increased energy shield")
        || label_lower.contains("evasion rating")
        || label_lower.contains("armour")
    {
        return (
            5,
            "Defence rolls are useful when pricing gear bases and upgrades.".to_string(),
        );
    }

    if label_lower.contains("spirit") || label_lower.contains("attribute") {
        return (
            5,
            "Attribute-style utility rolls can materially affect value.".to_string(),
        );
    }

    if label_lower.contains("damage")
        || label_lower.contains("attack speed")
        || label_lower.contains("critical")
    {
        return (
            5,
            "Offensive rolls are useful comparison stats.".to_string(),
        );
    }

    if label_lower.contains("stun threshold") || label_lower.contains("poison duration") {
        let direction = if max_value.is_some() {
            " Lower-is-better range is represented as a max-value trade filter."
        } else {
            ""
        };
        return (
            3,
            format!(
                "This mapped utility roll is searchable but lower priority for Quick Price.{direction}"
            ),
        );
    }

    (
        3,
        "Mapped modifier can be searched and is available for exact pricing.".to_string(),
    )
}

fn scored_reason(base_score: u8, bonus: u8, reason: &str) -> (u8, String) {
    let score = base_score.saturating_add(bonus).min(10);
    if bonus > 0 {
        (
            score,
            format!("{reason} high-roll bonus +{bonus} was applied."),
        )
    } else {
        (score, reason.to_string())
    }
}

fn pricing_roll_bonus(value: Option<f64>, thresholds: &[f64]) -> u8 {
    let Some(value) = value else {
        return 0;
    };

    thresholds
        .iter()
        .filter(|threshold| value >= **threshold)
        .count()
        .min(2) as u8
}

fn is_total_elemental_resistance_spec(spec: &TradeFilterSpec) -> bool {
    matches!(
        &spec.kind,
        TradeFilterKind::Stat { stat_id, .. }
            if stat_id == "pseudo.pseudo_total_elemental_resistance"
    )
}

fn is_individual_elemental_resistance_spec(spec: &TradeFilterSpec) -> bool {
    matches!(
        &spec.kind,
        TradeFilterKind::Stat { stat_id, .. }
            if matches!(
                stat_source(stat_id),
                "explicit" | "crafted" | "fractured" | "rune" | "desecrated"
            ) && matches!(
                stat_base_id(stat_id),
                "stat_3372524247" | "stat_4220027924" | "stat_1671376347"
            )
    )
}

fn category_profile_ids(item: &CapturedItem) -> Vec<String> {
    if is_gem_item_class(item.item_class.as_deref()) {
        profile_ids(&["quick", "exact"])
    } else {
        profile_ids(&["quick", "crafting-base", "exact"])
    }
}

fn profile_ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

pub fn socket_count(sockets: &str) -> Option<u32> {
    let count = sockets
        .split_whitespace()
        .filter(|part| part.chars().any(|ch| ch.is_ascii_alphanumeric()))
        .count();

    (count > 0).then_some(count as u32)
}

pub fn is_gem_item_class(item_class: Option<&str>) -> bool {
    item_class.map(normalized_item_class).is_some_and(|class| {
        matches!(
            class.as_str(),
            "skill gems" | "skill gem" | "active skill gems" | "support gems" | "support gem"
        )
    })
}

pub fn gem_level(item: &CapturedItem) -> Option<u32> {
    if !is_gem_item_class(item.item_class.as_deref()) {
        return None;
    }

    item.properties
        .iter()
        .find(|property| property.name.eq_ignore_ascii_case("Level"))
        .and_then(|property| parse_first_number(&property.value))
        .filter(|value| *value >= 0.0)
        .map(|value| value as u32)
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

pub fn should_show_unsupported_modifier(text: &str) -> bool {
    match clean_modifier_search_text(text) {
        Some(search_text) => search_text.chars().any(|ch| ch.is_ascii_digit()),
        None => false,
    }
}

fn normalized_modifier_text(text: &str) -> String {
    expand_trade_text_tags(text)
}

fn mapped_official_stat(
    text: &str,
    item_class: Option<&str>,
) -> Option<(String, String, Option<f64>)> {
    let search_text = clean_modifier_search_text(text)?;
    let comparable_modifier = comparable_modifier_text(&search_text);
    let preferred_sources = preferred_stat_sources(text);
    let mut best = None;

    for pattern in STAT_PATTERNS {
        let comparable_pattern = comparable_pattern_text(pattern.text);
        let Some(value) = match_stat_template(&comparable_pattern, &comparable_modifier) else {
            continue;
        };
        let rank = stat_source_rank(stat_source(pattern.id), &preferred_sources) * 10
            + local_stat_rank(pattern.text, &search_text, item_class);

        if best
            .as_ref()
            .map_or(true, |(best_rank, _, _, _)| rank < *best_rank)
        {
            best = Some((rank, pattern.id, pattern.text, value));

            if rank == 0 {
                break;
            }
        }
    }

    best.map(|(_, stat_id, pattern_text, value)| {
        (
            stat_id.to_string(),
            stat_filter_label(&search_text, pattern_text, value),
            value,
        )
    })
}

fn mapped_reduced_poison_duration(text: &str) -> Option<(String, String, f64)> {
    let search_text = clean_modifier_search_text(text)?;
    let comparable_modifier = comparable_modifier_text(&search_text);
    let comparable_pattern = comparable_pattern_text("#% reduced Poison Duration on you");
    let value = match_stat_template(&comparable_pattern, &comparable_modifier)??;
    let display_value = format_filter_value(value);

    Some((
        "explicit.stat_3301100256".to_string(),
        format!("Reduced Poison Duration on you: {display_value}%+ reduction"),
        -value,
    ))
}

fn mapped_charm_slots(text: &str) -> Option<(String, String, f64)> {
    let search_text = clean_modifier_search_text(text)?;
    let comparable_modifier = comparable_modifier_text(&search_text);
    let value = ["Has # Charm Slot", "Has # Charm Slots"]
        .into_iter()
        .find_map(|pattern| {
            let comparable_pattern = comparable_pattern_text(pattern);
            match_stat_template(&comparable_pattern, &comparable_modifier)
        })??;
    let display_value = format_filter_value(value);

    Some((
        "explicit.stat_1416292992".to_string(),
        format!("Charm Slots: {display_value}+"),
        value,
    ))
}

fn clean_modifier_search_text(text: &str) -> Option<String> {
    let expanded = expand_trade_text_tags(text).replace('\u{2019}', "'");
    let unscalable_stripped = strip_unscalable_suffix(&expanded);
    let source_stripped = strip_source_suffix(&unscalable_stripped);
    let trimmed = source_stripped.trim();

    if trimmed.is_empty() || is_modifier_section_marker(trimmed) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn strip_unscalable_suffix(text: &str) -> String {
    let mut result = text.trim().to_string();

    for suffix in [
        " -- Unscalable Value",
        " - Unscalable Value",
        " \u{2014} Unscalable Value",
    ] {
        if result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
            break;
        }
    }

    result
}

fn strip_source_suffix(text: &str) -> String {
    let mut result = text.trim().to_string();

    loop {
        let lower = result.to_lowercase();
        let mut stripped = false;

        for suffix in [
            " (rune)",
            " (crafted)",
            " (implicit)",
            " (enchant)",
            " (desecrated)",
            " (fractured)",
            " (sanctum)",
        ] {
            if lower.ends_with(suffix) {
                result.truncate(result.len() - suffix.len());
                result = result.trim_end().to_string();
                stripped = true;
                break;
            }
        }

        if !stripped {
            break;
        }
    }

    result
}

fn is_modifier_section_marker(text: &str) -> bool {
    text.starts_with('{') && text.ends_with('}')
}

fn comparable_pattern_text(text: &str) -> String {
    let comparable = comparable_stat_text(&expand_trade_text_tags(text).replace("+#", "#"));
    comparable
        .strip_suffix(" (local)")
        .unwrap_or(&comparable)
        .to_string()
}

fn comparable_modifier_text(text: &str) -> String {
    comparable_stat_text(text)
}

fn comparable_stat_text(text: &str) -> String {
    text.replace('\u{2019}', "'")
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn expand_trade_text_tags(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch != '[' {
            result.push(ch);
            continue;
        }

        let mut tag = String::new();
        let mut closed = false;

        for tag_ch in chars.by_ref() {
            if tag_ch == ']' {
                closed = true;
                break;
            }
            tag.push(tag_ch);
        }

        if closed {
            if let Some((_, display)) = tag.split_once('|') {
                result.push_str(display);
            } else {
                result.push_str(&tag);
            }
        } else {
            result.push('[');
            result.push_str(&tag);
        }
    }

    result
}

fn trade_text_segments(text: &str) -> Vec<TradeTextSegment> {
    let mut segments = Vec::new();
    let mut plain = String::new();
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch != '[' {
            plain.push(ch);
            continue;
        }

        let mut tag = String::new();
        let mut closed = false;

        for tag_ch in chars.by_ref() {
            if tag_ch == ']' {
                closed = true;
                break;
            }
            tag.push(tag_ch);
        }

        if !closed {
            plain.push('[');
            plain.push_str(&tag);
            continue;
        }

        if !plain.is_empty() {
            segments.push(TradeTextSegment {
                text: std::mem::take(&mut plain),
                tag: None,
                label: None,
                description: None,
                category: None,
            });
        }

        let (tag_name, display) = tag
            .split_once('|')
            .map_or((tag.as_str(), tag.as_str()), |(tag_name, display)| {
                (tag_name, display)
            });
        let keyword = describe_keyword(tag_name);
        segments.push(TradeTextSegment {
            text: display.to_string(),
            tag: Some(tag_name.to_string()),
            label: keyword.map(|keyword| keyword.label.to_string()),
            description: keyword.map(|keyword| keyword.description.to_string()),
            category: keyword.map(|keyword| keyword.category.to_string()),
        });
    }

    if !plain.is_empty() {
        segments.push(TradeTextSegment {
            text: plain,
            tag: None,
            label: None,
            description: None,
            category: None,
        });
    }

    segments
}

fn preferred_stat_sources(text: &str) -> Vec<&'static str> {
    let lower = text.to_lowercase();

    if lower.contains("(rune)") {
        vec!["rune", "explicit"]
    } else if lower.contains("(crafted)") {
        vec!["crafted", "explicit"]
    } else if lower.contains("(implicit)") {
        vec!["implicit", "explicit"]
    } else if lower.contains("(enchant)") {
        vec!["enchant", "explicit"]
    } else if lower.contains("(desecrated)") {
        vec!["desecrated", "explicit"]
    } else if lower.contains("(fractured)") {
        vec!["fractured", "explicit"]
    } else if lower.contains("(sanctum)") {
        vec!["sanctum", "explicit"]
    } else {
        vec![
            "explicit",
            "implicit",
            "rune",
            "crafted",
            "fractured",
            "desecrated",
            "enchant",
            "sanctum",
            "pseudo",
        ]
    }
}

fn stat_source(stat_id: &str) -> &str {
    stat_id
        .split_once('.')
        .map(|(source, _)| source)
        .unwrap_or_default()
}

fn stat_base_id(stat_id: &str) -> &str {
    stat_id.split_once('.').map(|(_, id)| id).unwrap_or(stat_id)
}

fn stat_source_rank(source: &str, preferred_sources: &[&str]) -> usize {
    if let Some(index) = preferred_sources
        .iter()
        .position(|preferred_source| *preferred_source == source)
    {
        return index;
    }

    match source {
        "explicit" => 20,
        "implicit" => 21,
        "rune" => 22,
        "crafted" => 23,
        "fractured" => 24,
        "desecrated" => 25,
        "enchant" => 26,
        "sanctum" => 27,
        "pseudo" => 28,
        _ => 99,
    }
}

fn local_stat_rank(pattern_text: &str, modifier_text: &str, item_class: Option<&str>) -> usize {
    let pattern_is_local = comparable_stat_text(pattern_text).ends_with(" (local)");
    let prefer_local =
        item_class.is_some_and(is_armour_item_class) && is_local_defence_modifier(modifier_text);

    match (pattern_is_local, prefer_local) {
        (true, true) | (false, false) => 0,
        (false, true) | (true, false) => 1,
    }
}

fn is_armour_item_class(item_class: &str) -> bool {
    matches!(
        item_class,
        "Body Armours" | "Boots" | "Gloves" | "Helmets" | "Shields"
    )
}

fn is_local_defence_modifier(text: &str) -> bool {
    let lower = text.to_lowercase();

    !lower.contains("global")
        && (lower.contains("armour")
            || lower.contains("evasion rating")
            || lower.contains("energy shield"))
}

fn match_stat_template(pattern: &str, modifier: &str) -> Option<Option<f64>> {
    if !pattern.contains('#') {
        return (pattern == modifier).then_some(None);
    }

    let parts = pattern.split('#').collect::<Vec<_>>();
    let mut position = 0;
    let mut first_value = None;

    if !match_literal_at(modifier, &mut position, parts[0]) {
        return None;
    }

    for part in parts.iter().skip(1) {
        let (value, next_position) = parse_template_number(modifier, position)?;
        first_value.get_or_insert(value);
        position = next_position;

        if !match_literal_at(modifier, &mut position, part) {
            return None;
        }
    }

    (position == modifier.len()).then_some(first_value)
}

fn match_literal_at(text: &str, position: &mut usize, literal: &str) -> bool {
    if literal.is_empty() {
        return true;
    }

    match text.get(*position..) {
        Some(remaining) if remaining.starts_with(literal) => {
            *position += literal.len();
            true
        }
        _ => false,
    }
}

fn parse_template_number(text: &str, position: usize) -> Option<(f64, usize)> {
    let bytes = text.as_bytes();
    let mut index = position;

    if index >= bytes.len() {
        return None;
    }

    let number_start = index;
    if bytes[index] == b'+' || bytes[index] == b'-' {
        index += 1;
    }

    let digit_start = index;
    let mut has_digit = false;

    while index < bytes.len() {
        match bytes[index] {
            b'0'..=b'9' => {
                has_digit = true;
                index += 1;
            }
            b'.' | b',' => {
                index += 1;
            }
            _ => break,
        }
    }

    if !has_digit || digit_start == index {
        return None;
    }

    let raw_number = text[number_start..index].replace('+', "").replace(',', "");
    let value = raw_number.parse::<f64>().ok()?;

    if text
        .get(index..)
        .is_some_and(|remaining| remaining.starts_with('('))
    {
        if let Some(offset) = text[index..].find(')') {
            index += offset + 1;
        }
    }

    Some((value, index))
}

fn stat_filter_label(modifier_text: &str, _pattern_text: &str, value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{}: {}+", modifier_text, format_filter_value(value)),
        None => modifier_text.to_string(),
    }
}

fn is_explicit_stat_spec(spec: &TradeFilterSpec) -> bool {
    matches!(
        &spec.kind,
        TradeFilterKind::Stat { stat_id, .. } if stat_id.starts_with("explicit.")
    )
}

fn category_for_item_class(item_class: &str) -> Option<&'static str> {
    let normalized = normalized_item_class(item_class);
    match normalized.as_str() {
        "boots" => Some("armour.boots"),
        "body armours" => Some("armour.chest"),
        "gloves" => Some("armour.gloves"),
        "helmets" => Some("armour.helmet"),
        "shields" => Some("armour.shield"),
        "amulets" => Some("accessory.amulet"),
        "rings" => Some("accessory.ring"),
        "belts" => Some("accessory.belt"),
        "skill gems" | "skill gem" | "active skill gems" => Some("gem.activegem"),
        "support gems" | "support gem" => Some("gem.supportgem"),
        _ => None,
    }
}

fn normalized_item_class(item_class: &str) -> String {
    item_class
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
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
    hideout_token: Option<String>,
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
    use crate::models::{
        CapturedItem, FilterValueOverride, TradeListing, TradeListingItem, TradePrice,
        TradeTextSegment,
    };
    use crate::parser::parse_item_text;
    use crate::trade::{
        build_fetch_url, build_trade_query, format_trade_api_error, is_blocked_or_rate_limited,
        map_fetch_response,
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

    const UNIQUE_BODY_ARMOUR_WITH_RANGED_STATS: &str = "Item Class: Body Armours
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
+17(17-18)% to all Elemental Resistances
+2 to Level of all Melee Skills";

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

    const RARE_WITH_AFFIX_MARKERS: &str = "Item Class: Body Armours
Rarity: Rare
Dread Shelter
Expert Hexer's Robe
--------
Item Level: 72
--------
{ Prefix Modifier }
+78 to maximum Life
{ Suffix Modifier }
+34% to Fire Resistance
{ Suffix Modifier }
+29% to Lightning Resistance";

    const ACTIVE_SKILL_GEM: &str = "Item Class: Skill Gems
Rarity: Gem
Spark
--------
Level: 15
Quality: +20%
--------
Item Level: 15";

    const SUPPORT_GEM: &str = "Item Class: Support Gems
Rarity: Gem
Persistence
--------
Level: 2
Quality: +10%
--------
Item Level: 2";

    fn empty_item() -> CapturedItem {
        CapturedItem {
            raw_text: String::new(),
            item_class: None,
            rarity: None,
            item_name: None,
            base_type: None,
            item_level: None,
            quality: None,
            sockets: None,
            properties: Vec::new(),
            explicit_mods: Vec::new(),
        }
    }

    fn trade_listing(id: &str, explicit_mods: &[&str]) -> TradeListing {
        TradeListing {
            id: id.to_string(),
            indexed: None,
            price: Some(TradePrice {
                price_type: Some("~b/o".to_string()),
                amount: 1.0,
                currency: "exalted".to_string(),
            }),
            account_name: None,
            can_teleport: true,
            hideout_token: None,
            item: TradeListingItem {
                icon: None,
                name: None,
                type_line: None,
                base_type: None,
                rarity: None,
                item_level: None,
                explicit_mods: explicit_mods
                    .iter()
                    .map(|modifier| modifier.to_string())
                    .collect(),
                pseudo_mods: Vec::new(),
                explicit_mod_segments: Vec::new(),
                pseudo_mod_segments: Vec::new(),
            },
        }
    }

    #[test]
    fn query_builder_includes_only_selected_supported_filters() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("item should parse");
        let query = build_trade_query(
            &item,
            &["identity:type".to_string(), "misc:item_level".to_string()],
        )
        .expect("query should build");

        assert_eq!(query["query"]["type"], "Expert Hexer's Robe");
        assert_eq!(
            query["query"]["filters"]["misc_filters"]["filters"]["ilvl"]["min"],
            72
        );
        assert!(query["query"]["stats"]
            .as_array()
            .expect("stats array")
            .is_empty());
        assert_eq!(
            query["query"]["filters"]["trade_filters"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn query_builder_applies_selected_filter_value_overrides() {
        let item = parse_item_text(RARE_BOOTS).expect("boots should parse");
        let query = super::build_trade_query_with_values(
            &item,
            &[
                "stat:explicit.stat_4052037485:0".to_string(),
                "stat:explicit.stat_4015621042:1".to_string(),
            ],
            &[
                FilterValueOverride {
                    id: "stat:explicit.stat_4052037485:0".to_string(),
                    min: Some(50.0),
                    max: Some(60.0),
                },
                FilterValueOverride {
                    id: "stat:explicit.stat_4015621042:1".to_string(),
                    min: None,
                    max: Some(140.0),
                },
            ],
        )
        .expect("query should build");

        let filters = query["query"]["stats"][0]["filters"]
            .as_array()
            .expect("stat filters");

        assert_eq!(filters[0]["value"]["min"], 50);
        assert_eq!(filters[0]["value"]["max"], 60);
        assert_eq!(filters[1]["value"]["min"], 124);
        assert_eq!(filters[1]["value"]["max"], 140);
    }

    #[test]
    fn query_builder_accepts_exact_selected_explicit_affixes_filter() {
        let item = parse_item_text(RARE_BOOTS).expect("boots should parse");
        let query = build_trade_query(
            &item,
            &[
                "identity:rarity".to_string(),
                "stat:explicit.stat_4052037485:0".to_string(),
                "stat:explicit.stat_4015621042:1".to_string(),
                "misc:exact_selected_explicit_affixes".to_string(),
            ],
        )
        .expect("query should build");

        let exact_count = super::selected_exact_explicit_affix_count(
            &item,
            &[
                "identity:rarity".to_string(),
                "stat:explicit.stat_4052037485:0".to_string(),
                "stat:explicit.stat_4015621042:1".to_string(),
                "misc:exact_selected_explicit_affixes".to_string(),
            ],
        )
        .expect("exact count should be understood");

        assert_eq!(exact_count, Some(2));
        assert_eq!(
            query["query"]["stats"][0]["filters"][2]["id"],
            "pseudo.pseudo_number_of_affix_mods"
        );
        assert_eq!(query["query"]["stats"][0]["filters"][2]["value"]["min"], 2);
        assert_eq!(query["query"]["stats"][0]["filters"][2]["value"]["max"], 2);
    }

    #[test]
    fn query_builder_accepts_exact_selected_prefix_and_suffix_affix_filters() {
        let item = parse_item_text(RARE_WITH_AFFIX_MARKERS).expect("rare item should parse");
        let query = build_trade_query(
            &item,
            &[
                "stat:explicit.stat_3299347043:0".to_string(),
                "stat:explicit.stat_3372524247:1".to_string(),
                "misc:exact_selected_prefix_affixes".to_string(),
                "misc:exact_selected_suffix_affixes".to_string(),
            ],
        )
        .expect("query should build");

        let filters = query["query"]["stats"][0]["filters"]
            .as_array()
            .expect("stat filters");
        let observed = filters
            .iter()
            .map(|filter| {
                (
                    filter["id"].as_str().expect("id"),
                    filter["value"]["min"].as_i64().expect("min"),
                    filter["value"]["max"].as_i64(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![
                ("explicit.stat_3299347043", 78, None),
                ("explicit.stat_3372524247", 34, None),
                ("pseudo.pseudo_number_of_prefix_mods", 1, Some(1)),
                ("pseudo.pseudo_number_of_suffix_mods", 1, Some(1)),
            ]
        );
    }

    #[test]
    fn exact_selected_explicit_affixes_requires_explicit_stats() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("body armour should parse");
        let error = super::selected_exact_explicit_affix_count(
            &item,
            &["misc:exact_selected_explicit_affixes".to_string()],
        )
        .expect_err("exact affix filter should need at least one explicit stat");

        assert!(error.contains("at least one explicit modifier"));
    }

    #[test]
    fn exact_selected_explicit_affixes_filters_fetched_listings_by_mod_count() {
        let one_mod = trade_listing("one-mod", &["12% increased maximum Energy Shield"]);
        let two_mods = trade_listing(
            "two-mods",
            &[
                "12% increased maximum Energy Shield",
                "10% increased Critical Hit Chance",
            ],
        );
        let no_mods = trade_listing("no-mods", &[]);

        let listings = super::apply_exact_selected_explicit_affix_filter(
            vec![one_mod, two_mods, no_mods],
            Some(1),
        );

        assert_eq!(
            listings
                .iter()
                .map(|listing| listing.id.as_str())
                .collect::<Vec<_>>(),
            vec!["one-mod"]
        );
    }

    #[test]
    fn query_builder_supports_active_gem_base_rarity_category_level_and_quality() {
        let item = parse_item_text(ACTIVE_SKILL_GEM).expect("skill gem should parse");
        let query = build_trade_query(
            &item,
            &[
                "category:gem.activegem".to_string(),
                "identity:type".to_string(),
                "identity:rarity".to_string(),
                "property:gem_level".to_string(),
                "property:quality".to_string(),
            ],
        )
        .expect("query should build");

        assert_eq!(query["query"]["type"], "Spark");
        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"],
            "gem.activegem"
        );
        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["rarity"]["option"],
            "gem"
        );
        assert_eq!(
            query["query"]["filters"]["misc_filters"]["filters"]["gem_level"]["min"],
            15
        );
        assert_eq!(
            query["query"]["filters"]["misc_filters"]["filters"]["quality"]["min"],
            20
        );
    }

    #[test]
    fn query_builder_supports_support_gem_category() {
        let item = parse_item_text(SUPPORT_GEM).expect("support gem should parse");
        let query = build_trade_query(&item, &["category:gem.supportgem".to_string()])
            .expect("query should build");

        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"],
            "gem.supportgem"
        );
    }

    #[test]
    fn query_builder_rejects_unknown_selected_filter_ids() {
        let item = parse_item_text(RARE_BODY_ARMOUR).expect("item should parse");
        let error = build_trade_query(
            &item,
            &[
                "identity:type".to_string(),
                "stat:explicit.stat_not_real:99".to_string(),
                "explicit:0".to_string(),
            ],
        )
        .expect_err("unknown selected ids should fail before API search");

        assert!(error.contains("Unknown selected filter IDs"));
        assert!(error.contains("stat:explicit.stat_not_real:99"));
        assert!(error.contains("explicit:0"));
    }

    #[test]
    fn api_error_message_lists_selected_filter_and_stat_ids() {
        let item = parse_item_text(RARE_BOOTS).expect("boots should parse");
        let selected_filter_ids = vec![
            "category:armour.boots".to_string(),
            "stat:explicit.stat_4052037485:0".to_string(),
            "stat:explicit.stat_4015621042:1".to_string(),
        ];
        let message = format_trade_api_error(
            "400 Bad Request",
            r#"{"error":{"code":2,"message":"Unknown stat id"}}"#,
            &item,
            &selected_filter_ids,
        );

        assert!(message.contains("Unknown stat id"));
        assert!(message.contains("Selected filter IDs: category:armour.boots"));
        assert!(message.contains("stat:explicit.stat_4052037485:0"));
        assert!(message.contains("Sent stat IDs:"));
        assert!(message.contains("explicit.stat_4052037485"));
        assert!(message.contains("explicit.stat_4015621042"));
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

        assert_eq!(query["query"]["status"]["option"], "securable");
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
        let query =
            build_trade_query(&item, &["identity:type".to_string()]).expect("query should build");

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
                "stat:explicit.stat_3984865854:1".to_string(),
                "stat:explicit.stat_4010677958:2".to_string(),
                "stat:explicit.stat_1379411836:3".to_string(),
                "stat:explicit.stat_1067622524:4".to_string(),
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
    fn query_builder_maps_official_stat_templates_with_copied_ranges() {
        let item = parse_item_text(UNIQUE_BODY_ARMOUR_WITH_RANGED_STATS)
            .expect("unique body armour should parse");
        let query = build_trade_query(
            &item,
            &[
                "stat:explicit.stat_2954116742|11184:0".to_string(),
                "stat:explicit.stat_124859000:1".to_string(),
                "stat:explicit.stat_3299347043:2".to_string(),
                "stat:explicit.stat_2901986750:3".to_string(),
                "stat:explicit.stat_9187492:4".to_string(),
            ],
        )
        .expect("query should build");

        let filters = query["query"]["stats"][0]["filters"]
            .as_array()
            .expect("stat filters");
        let observed = filters
            .iter()
            .map(|filter| {
                (
                    filter["id"].as_str().expect("id"),
                    filter["value"]["min"].as_f64(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![
                ("explicit.stat_2954116742|11184", None),
                ("explicit.stat_124859000", Some(40.0)),
                ("explicit.stat_3299347043", Some(114.0)),
                ("explicit.stat_2901986750", Some(17.0)),
                ("explicit.stat_9187492", Some(2.0)),
            ]
        );
    }

    #[test]
    fn query_builder_accepts_quick_sapphire_jewel_filters() {
        let item = empty_item();
        let query = build_trade_query(
            &item,
            &[
                "quick:jewel:sapphire:base".to_string(),
                "quick:jewel:sapphire:stat:explicit.stat_2482852589".to_string(),
                "quick:jewel:sapphire:stat:explicit.stat_2527686725".to_string(),
                "quick:jewel:sapphire:stat:explicit.stat_3556824919".to_string(),
                "quick:jewel:sapphire:stat:explicit.stat_587431675".to_string(),
            ],
        )
        .expect("quick jewel query should build");

        assert_eq!(query["query"]["type"], "Sapphire");
        assert_eq!(
            query["query"]["filters"]["type_filters"]["filters"]["category"]["option"],
            "jewel"
        );

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
                ("explicit.stat_2482852589", 15),
                ("explicit.stat_2527686725", 10),
                ("explicit.stat_3556824919", 10),
                ("explicit.stat_587431675", 10),
            ]
        );
    }

    #[test]
    fn query_builder_maps_sockets_and_reduced_poison_duration() {
        let item = parse_item_text(RARE_WITH_SOCKETS_AND_REDUCED_POISON_DURATION)
            .expect("item should parse");
        let query = build_trade_query(
            &item,
            &[
                "property:sockets".to_string(),
                "stat:explicit.stat_3301100256:0".to_string(),
            ],
        )
        .expect("query should build");

        assert_eq!(
            query["query"]["filters"]["equipment_filters"]["filters"]["rune_sockets"]["min"],
            2
        );
        assert_eq!(
            query["query"]["filters"]["equipment_filters"]["disabled"],
            false
        );

        let filter = &query["query"]["stats"][0]["filters"][0];
        assert_eq!(filter["id"], "explicit.stat_3301100256");
        assert_eq!(filter["value"]["max"], -59);
    }

    #[test]
    fn query_builder_maps_ranged_charm_slots() {
        let item = parse_item_text(RARE_BELT_WITH_CHARM_SLOTS).expect("item should parse");
        let query = build_trade_query(&item, &["stat:explicit.stat_1416292992:3".to_string()])
            .expect("query should build");

        let filter = &query["query"]["stats"][0]["filters"][0];
        assert_eq!(filter["id"], "explicit.stat_1416292992");
        assert_eq!(filter["value"]["min"], 2);
    }

    #[test]
    fn query_builder_maps_empty_affix_pseudo_filters() {
        let item = parse_item_text(RARE_WITH_AFFIX_MARKERS).expect("rare item should parse");
        let query = build_trade_query(
            &item,
            &[
                "stat:pseudo.pseudo_number_of_empty_prefix_mods".to_string(),
                "stat:pseudo.pseudo_number_of_empty_suffix_mods".to_string(),
            ],
        )
        .expect("query should build");

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
                ("pseudo.pseudo_number_of_empty_prefix_mods", 2),
                ("pseudo.pseudo_number_of_empty_suffix_mods", 1),
            ]
        );

        let pseudos = super::selected_pseudo_stat_ids(
            &item,
            &["stat:pseudo.pseudo_number_of_empty_suffix_mods".to_string()],
        );
        assert_eq!(pseudos, vec!["pseudo.pseudo_number_of_empty_suffix_mods"]);
    }

    #[test]
    fn fetch_url_includes_result_ids_query_realm_and_pseudos() {
        let ids = (0..10)
            .map(|index| format!("id{index}"))
            .collect::<Vec<_>>();
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
    fn trade_text_segments_enriches_curse_keyword() {
        let segments = super::trade_text_segments("[Curse]");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Curse");
        assert_eq!(segments[0].tag.as_deref(), Some("Curse"));
        assert_eq!(segments[0].label.as_deref(), Some("Curse"));
        assert_eq!(segments[0].category.as_deref(), Some("ailment-debuff"));
        assert!(segments[0]
            .description
            .as_deref()
            .expect("curse description")
            .contains("debuff"));
    }

    #[test]
    fn trade_text_segments_preserves_alias_and_enriches_energy_shield_keyword() {
        let segments = super::trade_text_segments("[EnergyShield|Energy Shield]");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Energy Shield");
        assert_eq!(segments[0].tag.as_deref(), Some("EnergyShield"));
        assert_eq!(segments[0].label.as_deref(), Some("Energy Shield"));
        assert_eq!(segments[0].category.as_deref(), Some("defence"));
        assert!(segments[0]
            .description
            .as_deref()
            .expect("energy shield description")
            .contains("damage"));
    }

    #[test]
    fn trade_text_segments_keeps_unknown_keywords_serializable() {
        let segments = super::trade_text_segments("[FuturePoe2Thing]");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "FuturePoe2Thing");
        assert_eq!(segments[0].tag.as_deref(), Some("FuturePoe2Thing"));
        assert!(segments[0].label.is_none());
        assert!(segments[0].description.is_none());
        assert!(segments[0].category.is_none());

        let serialized =
            serde_json::to_string(&segments[0]).expect("unknown segment should serialize");
        assert!(serialized.contains("FuturePoe2Thing"));
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
        assert_eq!(
            listings[0].id,
            "87dc03118c0a90f95957ae9b5495f322d2de521879fb97d093ad6a71dafcde68"
        );
        assert_eq!(listings[0].price.as_ref().expect("price").amount, 5.0);
        assert_eq!(listings[0].price.as_ref().expect("price").currency, "chaos");
        assert_eq!(listings[0].account_name.as_deref(), Some("SGM#6552"));
        assert_eq!(listings[0].item.name.as_deref(), Some("Cataclysm Road"));
        assert_eq!(listings[0].item.item_level, Some(82));
        assert_eq!(listings[0].item.explicit_mods.len(), 2);
        assert_eq!(
            listings[0].item.explicit_mods,
            vec![
                "124% increased Energy Shield".to_string(),
                "+371 to Stun Threshold".to_string()
            ]
        );
        assert_eq!(
            listings[0].item.explicit_mod_segments[0],
            vec![
                TradeTextSegment {
                    text: "124% increased ".to_string(),
                    tag: None,
                    label: None,
                    description: None,
                    category: None,
                },
                TradeTextSegment {
                    text: "Energy Shield".to_string(),
                    tag: Some("EnergyShield".to_string()),
                    label: Some("Energy Shield".to_string()),
                    description: Some(
                        "A protective resource that absorbs damage before Life until depleted."
                            .to_string()
                    ),
                    category: Some("defence".to_string()),
                },
            ]
        );
        assert_eq!(
            listings[0].item.pseudo_mods,
            vec!["+45% total Elemental Resistance"]
        );
        assert!(listings[0].can_teleport);
        assert_eq!(listings[0].hideout_token.as_deref(), Some("secret-token"));

        let serialized = serde_json::to_string(&listings[0]).expect("listing should serialize");
        assert!(serialized.contains("\"canTeleport\":true"));
        assert!(!serialized.contains("hideout_token"));
        assert!(!serialized.contains("secret-token"));
    }

    #[test]
    fn fetch_response_mapper_marks_exact_buyout_without_token_teleportable_for_bridge_resolution() {
        let response = r#"{
            "result": [{
                "id": "no-token-listing",
                "listing": {
                    "indexed": "2026-07-04T14:26:51Z",
                    "price": { "type": "~b/o", "amount": 5, "currency": "chaos" },
                    "account": { "name": "SGM#6552", "online": null }
                },
                "item": {
                    "icon": "https://web.poecdn.com/image.png",
                    "name": "Cataclysm Road",
                    "typeLine": "Bound Sandals",
                    "baseType": "Bound Sandals",
                    "rarity": "Rare",
                    "ilvl": 82,
                    "explicitMods": [],
                    "pseudoMods": []
                }
            }]
        }"#;

        let listings = map_fetch_response(response).expect("fetch response should map");

        assert!(listings[0].can_teleport);
        assert!(listings[0].hideout_token.is_none());
    }

    #[test]
    fn fetch_response_mapper_only_marks_exact_buyout_listings_teleportable() {
        let response = r#"{
            "result": [
                {
                    "id": "exact-buyout",
                    "listing": {
                        "price": { "type": "~b/o", "amount": 1, "currency": "exalted" },
                        "hideout_token": "buyout-token"
                    },
                    "item": { "typeLine": "Sapphire", "explicitMods": [], "pseudoMods": [] }
                },
                {
                    "id": "fixed-price",
                    "listing": {
                        "price": { "type": "~price", "amount": 1, "currency": "exalted" },
                        "hideout_token": "fixed-price-token"
                    },
                    "item": { "typeLine": "Sapphire", "explicitMods": [], "pseudoMods": [] }
                }
            ]
        }"#;

        let listings = map_fetch_response(response).expect("fetch response should map");

        assert!(listings[0].can_teleport);
        assert!(!listings[1].can_teleport);
        assert!(listings[1].hideout_token.is_none());
    }

    #[test]
    fn api_block_and_rate_limit_statuses_are_detected() {
        assert!(is_blocked_or_rate_limited(403));
        assert!(is_blocked_or_rate_limited(429));
        assert!(!is_blocked_or_rate_limited(500));
        assert!(!is_blocked_or_rate_limited(200));
    }
}
