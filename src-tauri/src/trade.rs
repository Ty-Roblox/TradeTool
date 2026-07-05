use std::collections::HashSet;

use crate::models::{CapturedItem, TradeSearchResponse};
use serde::Deserialize;
use serde_json::{json, Value};

const TRADE_BASE_URL: &str = "https://www.pathofexile.com";

pub fn build_trade_query(item: &CapturedItem, selected_filter_ids: &[String]) -> Result<Value, String> {
    let selected = selected_filter_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut query = json!({
        "query": {
            "status": { "option": "online" },
            "stats": []
        },
        "sort": {
            "price": "asc"
        }
    });

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
    if status.as_u16() == 403 || status.as_u16() == 429 {
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

    Ok(TradeSearchResponse {
        url,
        search_id: body.id,
        query,
    })
}

fn sanitize_league(league: &str) -> Result<String, String> {
    let trimmed = league.trim();
    if trimmed.is_empty() {
        return Err("League is required.".to_string());
    }

    Ok(trimmed.replace(' ', "%20"))
}

#[derive(Debug, Deserialize)]
struct TradeSearchApiResponse {
    id: String,
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_item_text;
    use crate::trade::build_trade_query;

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
}
