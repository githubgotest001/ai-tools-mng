//! Grok Bot 周额度（内部名 Sand）。
//!
//! 官方 usage-summary / GetCurrentPeriodUsage 不含该字段。
//! 仪表盘用量条来自 `POST /api/dashboard/get-sand-usage-status`。

use crate::http_client::create_proxy_client;
use crate::platforms::cursor::modules::auth::{GrokBotUsage, IndividualUsage, UsageSummary};
use serde_json::Value;

const REQUEST_TIMEOUT_SECS: u64 = 8;
const SAND_USAGE_URL: &str = "https://cursor.com/api/dashboard/get-sand-usage-status";

pub fn has_grok_bot_meter(summary: &UsageSummary) -> bool {
    summary
        .individual_usage
        .as_ref()
        .and_then(|u| u.grok_bot.as_ref())
        .is_some_and(GrokBotUsage::has_meter)
}

pub fn attach_grok_bot(summary: &mut UsageSummary, bot: GrokBotUsage) {
    if !bot.has_meter() {
        return;
    }
    match summary.individual_usage.as_mut() {
        Some(usage) => usage.grok_bot = Some(bot),
        None => {
            summary.individual_usage = Some(IndividualUsage {
                plan: None,
                on_demand: None,
                billing_cycle_start: summary.billing_cycle_start.clone(),
                billing_cycle_end: summary.billing_cycle_end.clone(),
                grok_bot: Some(bot),
            });
        }
    }
}

/// 从任意 JSON 提取 Grok Bot 周额度（Sand 响应或嵌套 grokBot 对象）
pub fn extract_from_value(value: &Value) -> Option<GrokBotUsage> {
    walk_value(value, 0, "")
}

fn walk_value(value: &Value, depth: usize, parent_key: &str) -> Option<GrokBotUsage> {
    if depth > 8 {
        return None;
    }

    match value {
        Value::Object(map) => {
            if map.get("usesPooledEnterpriseAllowance").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            if let Some(bot) = parse_sand_usage(value) {
                return Some(bot);
            }
            if is_grok_bot_key(parent_key) || is_grok_bot_product(map) {
                if let Some(bot) = usage_from_object(value) {
                    return Some(bot);
                }
            }
            for (key, child) in map {
                if let Some(bot) = walk_value(child, depth + 1, key) {
                    return Some(bot);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(bot) = walk_value(item, depth + 1, parent_key) {
                    return Some(bot);
                }
            }
            None
        }
        _ => None,
    }
}

fn is_grok_bot_key(key: &str) -> bool {
    let compact = compact_alnum(key);
    compact.contains("grokbot")
        || compact == "botusage"
        || compact.contains("grokbotusage")
        || compact.contains("botpercentused")
}

fn is_grok_bot_product(map: &serde_json::Map<String, Value>) -> bool {
    const PRODUCT_KEYS: [&str; 5] = [
        "product",
        "productName",
        "name",
        "modelIntent",
        "grokPlanLabel",
    ];
    PRODUCT_KEYS.iter().any(|k| {
        map.get(*k)
            .and_then(Value::as_str)
            .is_some_and(is_grok_bot_name)
    })
}

fn is_grok_bot_name(name: &str) -> bool {
    let compact = compact_alnum(name);
    compact.contains("grokbot")
}

fn compact_alnum(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// 解析仪表盘 `get-sand-usage-status` 响应
fn parse_sand_usage(value: &Value) -> Option<GrokBotUsage> {
    let obj = value.as_object()?;
    if obj.get("usesPooledEnterpriseAllowance").and_then(Value::as_bool) == Some(true) {
        return None;
    }

    let sand_hint = [
        "grokPlanLabel",
        "nextResetTimestampUtc",
        "hasNonZeroIncludedLimit",
        "hasAvailableUsage",
        "sandTrialExpiresAt",
        "includedUsageSuperGrokPlan",
        "currentPeriodStart",
    ]
    .iter()
    .any(|k| obj.contains_key(*k));
    if !sand_hint {
        return None;
    }

    let percent_used = first_f64(obj, &["usagePercent", "percentUsed"]);
    let (period_start, period_end) = (
        timestamp_string(
            obj,
            &[
                "currentPeriodStart",
                "periodStart",
                "billingPeriodStart",
                "weekStart",
            ],
        ),
        timestamp_string(
            obj,
            &[
                "nextResetTimestampUtc",
                "periodEnd",
                "billingPeriodEnd",
                "weekEnd",
                "resetAt",
                "resetsAt",
            ],
        ),
    );

    let bot = GrokBotUsage {
        enabled: obj
            .get("hasNonZeroIncludedLimit")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("hasAvailableUsage").and_then(Value::as_bool))
            .unwrap_or(percent_used.is_some()),
        percent_used,
        used: None,
        limit: None,
        remaining: None,
        period_start,
        period_end,
    };
    bot.has_meter().then_some(bot)
}

fn usage_from_object(value: &Value) -> Option<GrokBotUsage> {
    let obj = value.as_object()?;

    let percent_used = first_f64(
        obj,
        &[
            "percentUsed",
            "usagePercent",
            "creditUsagePercent",
            "grokBotPercentUsed",
            "botPercentUsed",
            "percent_used",
            "usage_percent",
        ],
    )
    .or_else(|| percent_from_used_limit(obj));

    let remaining_percent = first_f64(
        obj,
        &[
            "percentRemaining",
            "remainingPercent",
            "percent_remaining",
            "remaining_percent",
        ],
    );

    let percent_used = percent_used.or_else(|| remaining_percent.map(|left| (100.0 - left).max(0.0)));

    let used = first_f64(obj, &["used", "includedUsed", "totalUsed"]);
    let limit = first_f64(obj, &["limit", "monthlyLimit", "includedLimit", "allowance"]);
    let remaining = first_f64(obj, &["remaining", "remainingAllowance"]);
    let (period_start, period_end) = period_bounds(obj);

    let bot = GrokBotUsage {
        enabled: obj
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(percent_used.is_some() || used.is_some() || limit.is_some()),
        percent_used,
        used,
        limit,
        remaining,
        period_start,
        period_end,
    };

    bot.has_meter().then_some(bot)
}

fn percent_from_used_limit(obj: &serde_json::Map<String, Value>) -> Option<f64> {
    let used = first_f64(obj, &["used", "includedUsed", "totalUsed"])?;
    let limit = first_f64(obj, &["limit", "monthlyLimit", "includedLimit", "allowance"])?;
    if limit <= 0.0 || !used.is_finite() || !limit.is_finite() {
        return None;
    }
    Some((used / limit * 100.0).max(0.0))
}

fn period_bounds(obj: &serde_json::Map<String, Value>) -> (Option<String>, Option<String>) {
    let period = obj
        .get("currentPeriod")
        .or_else(|| obj.get("period"))
        .and_then(Value::as_object);

    let start = timestamp_string(
        obj,
        &["periodStart", "billingPeriodStart", "weekStart", "start", "period_start"],
    )
    .or_else(|| period.and_then(|p| first_string(p, &["start", "periodStart"])));

    let end = timestamp_string(
        obj,
        &[
            "periodEnd",
            "billingPeriodEnd",
            "weekEnd",
            "end",
            "resetAt",
            "resetsAt",
            "period_end",
            "nextResetTimestampUtc",
        ],
    )
    .or_else(|| period.and_then(|p| first_string(p, &["end", "periodEnd"])));

    (start, end)
}

fn first_f64(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = value_as_f64(v) {
                if n.is_finite() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn first_string(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = timestamp_from_value(obj.get(*key)?) {
            return Some(s);
        }
    }
    None
}

fn timestamp_string(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    first_string(obj, keys)
}

fn timestamp_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => {
            let raw = n.as_f64()?;
            unix_ms_or_secs_to_iso(raw)
        }
        Value::Object(obj) => {
            if let Some(s) = obj.get("seconds").and_then(value_as_f64) {
                return unix_ms_or_secs_to_iso(s);
            }
            obj.get("val").and_then(timestamp_from_value)
        }
        _ => None,
    }
}

fn unix_ms_or_secs_to_iso(raw: f64) -> Option<String> {
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    let secs = if raw > 1_000_000_000_000.0 {
        raw / 1000.0
    } else {
        raw
    };
    let secs = secs as i64;
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        Value::Object(obj) => obj.get("val").and_then(value_as_f64),
        _ => None,
    }
}

/// 拉取 Grok Bot 周额度（session Cookie → Sand 接口）
pub async fn fetch_grok_bot_usage(session_token: &str) -> Option<GrokBotUsage> {
    let cookie = format!("WorkosCursorSessionToken={}", session_token);
    let json = post_json(SAND_USAGE_URL, &cookie, serde_json::json!({})).await?;
    parse_sand_usage(&json)
}

async fn post_json(url: &str, cookie: &str, body: Value) -> Option<Value> {
    let client = create_proxy_client().ok()?;
    let response = client
        .post(url)
        .header("Cookie", cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", "https://cursor.com")
        .header("Referer", "https://cursor.com/dashboard/usage")
        .json(&body)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().await.ok()?;
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_sand_usage_status() {
        let json = serde_json::json!({
            "currentPeriodStart": "2026-08-22T17:21:20.679Z",
            "nextResetTimestampUtc": "2026-08-29T17:21:20.679Z",
            "usagePercent": 13.588743,
            "hasAvailableUsage": true,
            "hasNonZeroIncludedLimit": true,
            "onDemandSettings": {
                "visible": true,
                "eligible": true,
                "dashboardUrl": "https://cursor.com/dashboard/spending"
            },
            "grokPlanLabel": "Grok Bot Plan"
        });
        let bot = extract_from_value(&json).unwrap();
        assert!((bot.percent_used.unwrap() - 13.588743).abs() < 1e-6);
        assert_eq!(bot.period_start.as_deref(), Some("2026-08-22T17:21:20.679Z"));
        assert_eq!(bot.period_end.as_deref(), Some("2026-08-29T17:21:20.679Z"));
        assert!(bot.enabled);
    }

    #[test]
    fn ignores_enterprise_pooled_allowance() {
        let json = serde_json::json!({
            "usagePercent": 10,
            "usesPooledEnterpriseAllowance": true,
            "grokPlanLabel": "Grok Bot Plan"
        });
        assert!(extract_from_value(&json).is_none());
    }

    #[test]
    fn extracts_nested_grok_bot_percent() {
        let json = serde_json::json!({
            "membershipType": "ultra",
            "individualUsage": {
                "plan": { "autoPercentUsed": 10, "apiPercentUsed": 20 },
                "grokBot": { "percentUsed": 37.4, "periodEnd": "2026-08-30T00:00:00Z" }
            }
        });
        let bot = extract_from_value(&json).unwrap();
        assert_eq!(bot.percent_used, Some(37.4));
        assert_eq!(bot.period_end.as_deref(), Some("2026-08-30T00:00:00Z"));
    }

    #[test]
    fn extracts_product_usage_array() {
        let json = serde_json::json!({
            "productUsage": [
                { "product": "Cursor", "usagePercent": 12 },
                { "product": "Grok Bot", "usagePercent": 55, "currentPeriod": { "end": "2026-08-29T12:00:00Z" } }
            ]
        });
        let bot = extract_from_value(&json).unwrap();
        assert_eq!(bot.percent_used, Some(55.0));
        assert!(bot.period_end.is_some());
    }

    #[test]
    fn ignores_unrelated_grok_model_names() {
        let json = serde_json::json!({
            "aggregations": [{ "modelIntent": "grok-4.6", "usagePercent": 99 }]
        });
        assert!(extract_from_value(&json).is_none());
    }

    #[test]
    fn computes_percent_from_used_and_limit() {
        let json = serde_json::json!({
            "grokBotUsage": { "used": 25, "limit": 100 }
        });
        let bot = extract_from_value(&json).unwrap();
        assert_eq!(bot.percent_used, Some(25.0));
    }
}
