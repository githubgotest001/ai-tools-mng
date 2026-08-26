//! Auto / API 两个用量池的本账期花费（美分）。
//!
//! usage-summary 只给出 autoPercentUsed / apiPercentUsed 两个百分比，没有金额；
//! 池预算也不在任何响应里。仪表盘 Spending 页的分池金额来自
//! `POST /api/dashboard/get-aggregated-usage-events`：每条 aggregation 带一个
//! `tier`，2 = Cursor Models（Auto / Composer / Grok），1 = Other Models（第三方）。
//! 按 tier 汇总 totalCents 就能拿到两池各自的花费，再配合百分比即可反推池预算
//! （实测两池花费之和与 usage-summary 的 `breakdown.total` 完全一致）。

use crate::http_client::create_proxy_client;
use crate::platforms::cursor::modules::auth::{PlanUsage, UsageSummary};
use serde_json::Value;

const REQUEST_TIMEOUT_SECS: u64 = 15;
const AGGREGATED_USAGE_URL: &str =
    "https://cursor.com/api/dashboard/get-aggregated-usage-events";

/// Cursor Models 池（Auto / Composer / Grok）
const TIER_CURSOR_MODELS: i64 = 2;
/// Other Models 池（第三方模型）
const TIER_OTHER_MODELS: i64 = 1;

/// 两个用量池在本账期的花费（美分）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoolSpend {
    pub auto_cents: f64,
    pub api_cents: f64,
}

/// 按 tier 汇总 aggregations 的 totalCents。
///
/// 只认已知的两个 tier：出现新 tier 时宁可漏算，也不要把它塞进某一池里
/// 让金额和百分比对不上。totalCents 缺失（当天还没结算）按 0 计。
pub fn sum_by_tier(response: &Value) -> Option<PoolSpend> {
    let aggregations = response.get("aggregations")?.as_array()?;

    let mut spend = PoolSpend {
        auto_cents: 0.0,
        api_cents: 0.0,
    };
    let mut matched = false;

    for item in aggregations {
        let tier = item.get("tier").and_then(Value::as_i64);
        let cents = item
            .get("totalCents")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite())
            .unwrap_or(0.0);
        match tier {
            Some(TIER_CURSOR_MODELS) => {
                spend.auto_cents += cents;
                matched = true;
            }
            Some(TIER_OTHER_MODELS) => {
                spend.api_cents += cents;
                matched = true;
            }
            _ => {}
        }
    }

    matched.then_some(spend)
}

/// 把分池花费写进 plan，供前端反推池预算
pub fn attach_pool_spend(summary: &mut UsageSummary, spend: PoolSpend) {
    if let Some(plan) = summary
        .individual_usage
        .as_mut()
        .and_then(|usage| usage.plan.as_mut())
    {
        plan.auto_spend_cents = Some(spend.auto_cents);
        plan.api_spend_cents = Some(spend.api_cents);
    }
}

/// plan 里是否已经有可用于反推的百分比（没有就不必多打一次请求）
pub fn plan_has_percent(plan: &PlanUsage) -> bool {
    plan.auto_percent_used.is_some()
        || plan.api_percent_used.is_some()
        || plan.total_percent_used.is_some()
}

/// 拉取本账期的分池花费。
///
/// `billing_cycle_start_ms` 为账期起点（毫秒）；不传 endDate，与仪表盘一致地
/// 取「账期开始至今」。任何失败都返回 None——分池金额只是锦上添花，不能让它
/// 拖垮整个配额刷新。
pub async fn fetch_pool_spend(session_token: &str, billing_cycle_start_ms: i64) -> Option<PoolSpend> {
    let client = create_proxy_client().ok()?;

    let response = client
        .post(AGGREGATED_USAGE_URL)
        .header(
            "Cookie",
            format!("WorkosCursorSessionToken={}", session_token),
        )
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", "https://cursor.com")
        .header("Referer", "https://cursor.com/dashboard/spending")
        // teamId -1 表示「只要当前个人账号」，与仪表盘 Spending 页一致
        .json(&serde_json::json!({ "teamId": -1, "startDate": billing_cycle_start_ms }))
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let text = response.text().await.ok()?;
    let json: Value = serde_json::from_str(text.trim_start()).ok()?;
    sum_by_tier(&json)
}

/// 把 ISO 时间戳转成毫秒
pub fn iso_to_millis(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_response() -> Value {
        // 取自真实响应：tier 1 = Other Models，tier 2 = Cursor Models
        serde_json::json!({
            "aggregations": [
                { "modelIntent": "claude-fable-5-thinking-xhigh", "totalCents": 12135.8534, "tier": 1 },
                { "modelIntent": "gpt-5.6-sol-medium", "totalCents": 4512.778365, "tier": 1 },
                { "modelIntent": "cursor-grok-4.5-high-fast", "totalCents": 2033.1491, "tier": 2 },
                { "modelIntent": "composer-2.5-fast", "totalCents": 1573.3641, "tier": 2 },
                { "modelIntent": "default", "totalCents": 537.846935, "tier": 2 },
                { "modelIntent": "cursor-grok-4.6-xhigh-fast", "totalCents": 450.51496, "tier": 2 },
                { "modelIntent": "cursor-grok-4.6-high", "totalCents": 75.26657, "tier": 2 },
                { "modelIntent": "claude-opus-5-thinking-high", "totalCents": 29.015825, "tier": 1 },
                { "modelIntent": "cursor-grok-4.5-high", "totalCents": 9.8836, "tier": 2 },
                // 当天尚未结算的行没有 totalCents，按 0 计而不是丢掉整个响应
                { "modelIntent": "claude-haiku-4-5", "tier": 1 }
            ]
        })
    }

    #[test]
    fn sums_cents_per_pool() {
        let spend = sum_by_tier(&live_response()).unwrap();
        assert!((spend.auto_cents - 4680.025265).abs() < 1e-6);
        assert!((spend.api_cents - 16677.647590).abs() < 1e-6);
        // 两池之和即 usage-summary 的 breakdown.total（实测一致）
        assert!((spend.auto_cents + spend.api_cents - 21357.672855).abs() < 1e-6);
    }

    #[test]
    fn unknown_tiers_are_not_folded_into_a_pool() {
        let json = serde_json::json!({
            "aggregations": [
                { "modelIntent": "a", "totalCents": 100.0, "tier": 2 },
                { "modelIntent": "future-pool", "totalCents": 999.0, "tier": 7 },
                { "modelIntent": "no-tier", "totalCents": 888.0 }
            ]
        });
        let spend = sum_by_tier(&json).unwrap();
        assert_eq!(spend.auto_cents, 100.0);
        assert_eq!(spend.api_cents, 0.0);
    }

    #[test]
    fn missing_or_untiered_aggregations_yield_nothing() {
        assert!(sum_by_tier(&serde_json::json!({})).is_none());
        assert!(sum_by_tier(&serde_json::json!({ "aggregations": [] })).is_none());
        assert!(
            sum_by_tier(&serde_json::json!({
                "aggregations": [{ "modelIntent": "a", "totalCents": 1.0 }]
            }))
            .is_none()
        );
    }

    #[test]
    fn attach_requires_an_existing_plan() {
        let spend = PoolSpend {
            auto_cents: 12.0,
            api_cents: 34.0,
        };

        // 没有 plan（团队/企业账号）时静默跳过，不能凭空造出一个 plan
        let mut empty: UsageSummary =
            serde_json::from_value(serde_json::json!({ "membershipType": "enterprise" })).unwrap();
        attach_pool_spend(&mut empty, spend);
        assert!(empty.individual_usage.is_none());

        let mut summary: UsageSummary = serde_json::from_value(serde_json::json!({
            "individualUsage": { "plan": { "enabled": true, "autoPercentUsed": 10.4 } }
        }))
        .unwrap();
        attach_pool_spend(&mut summary, spend);
        let plan = summary.individual_usage.unwrap().plan.unwrap();
        assert_eq!(plan.auto_spend_cents, Some(12.0));
        assert_eq!(plan.api_spend_cents, Some(34.0));
    }

    #[test]
    fn parses_billing_cycle_start_to_millis() {
        assert_eq!(
            iso_to_millis("2026-08-06T13:57:31.000Z"),
            Some(1786024651000)
        );
        assert_eq!(iso_to_millis("not-a-date"), None);
    }

    #[test]
    fn plan_without_percent_skips_the_extra_request() {
        let plan: PlanUsage =
            serde_json::from_value(serde_json::json!({ "enabled": true, "used": 100 })).unwrap();
        assert!(!plan_has_percent(&plan));

        let plan: PlanUsage =
            serde_json::from_value(serde_json::json!({ "totalPercentUsed": 43.0 })).unwrap();
        assert!(plan_has_percent(&plan));
    }
}
