use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapturedItem {
    pub raw_text: String,
    pub item_class: Option<String>,
    pub rarity: Option<String>,
    pub item_name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<u32>,
    pub quality: Option<i32>,
    pub sockets: Option<String>,
    pub properties: Vec<ItemProperty>,
    pub explicit_mods: Vec<ItemModifier>,
}

impl CapturedItem {
    pub fn empty() -> Self {
        Self {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemModifier {
    pub index: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affix_side: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FilterGroup {
    pub id: String,
    pub label: String,
    pub filters: Vec<FilterCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FilterCandidate {
    pub id: String,
    pub label: String,
    pub selected_by_default: bool,
    pub supported: bool,
    pub unsupported_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affix_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profile_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PriceCheckProfile {
    pub id: String,
    pub label: String,
    pub description: String,
    pub filter_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppDiagnostic {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResponse {
    pub hotkey: String,
    pub item: CapturedItem,
    pub filter_groups: Vec<FilterGroup>,
    pub price_check_profiles: Vec<PriceCheckProfile>,
    pub diagnostics: Vec<AppDiagnostic>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTradeRequest {
    pub league: String,
    pub raw_text: Option<String>,
    pub selected_filter_ids: Vec<String>,
    #[serde(default)]
    pub selected_filter_values: Vec<FilterValueOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FilterValueOverride {
    pub id: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeSearchResponse {
    pub url: String,
    pub search_id: String,
    pub total: usize,
    pub result_ids: Vec<String>,
    pub fetched_count: usize,
    pub listings: Vec<TradeListing>,
    pub query: serde_json::Value,
    pub fetch_url: Option<String>,
    pub warning: Option<String>,
    pub diagnostics: Vec<AppDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TradeListing {
    pub id: String,
    pub indexed: Option<String>,
    pub price: Option<TradePrice>,
    pub account_name: Option<String>,
    pub can_teleport: bool,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub hideout_token: Option<String>,
    pub item: TradeListingItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TradePrice {
    pub price_type: Option<String>,
    pub amount: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TradeListingItem {
    pub icon: Option<String>,
    pub name: Option<String>,
    pub type_line: Option<String>,
    pub base_type: Option<String>,
    pub rarity: Option<String>,
    pub item_level: Option<u32>,
    pub explicit_mods: Vec<String>,
    pub pseudo_mods: Vec<String>,
    #[serde(default)]
    pub explicit_mod_segments: Vec<Vec<TradeTextSegment>>,
    #[serde(default)]
    pub pseudo_mod_segments: Vec<Vec<TradeTextSegment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TradeTextSegment {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}
