//! Cursor 登录设备 / 活跃会话（对应仪表盘 Settings → Active Sessions）。
//!
//! 官方没有公开 API，这两个是 cursor.com 前端自己在调的私有接口，鉴权方式与
//! usage-summary 完全一致（`WorkosCursorSessionToken` Cookie）：
//!
//! - `GET  /api/auth/sessions`        -> `{ "sessions": [ ... ] }`，失败为 `{ "error": { "message" } }`
//! - `POST /api/auth/sessions/revoke` -> body `{ "session_id": <id>, "type": <数字枚举> }`
//!
//! 会话类型在列表里是字符串枚举，撤销时要换成数字，映射取自官方前端
//! `authSessionTypeRevokeValue`：WEB=1 / CLIENT=2 / MOBILE=10 / CHROME_EXTENSION=11。
//! 官方前端对未知类型会得到 `undefined` 并在序列化时把 `type` 整个丢掉，这里保持同样行为。
//!
//! 单条会话的字段名没有拿到真实账号实测过，所以不写死结构体：只按候选名抽出要用的几项，
//! 其余整个对象原样透传给前端展示，Cursor 改字段也不会把列表打空。

use crate::http_client::create_proxy_client;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SESSIONS_URL: &str = "https://cursor.com/api/auth/sessions";
const REVOKE_URL: &str = "https://cursor.com/api/auth/sessions/revoke";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// 会话类型字符串 -> 撤销接口要的数字枚举
fn revoke_type_value(session_type: &str) -> Option<i32> {
    match session_type {
        "SESSION_TYPE_WEB" => Some(1),
        "SESSION_TYPE_CLIENT" => Some(2),
        "SESSION_TYPE_MOBILE" => Some(10),
        "SESSION_TYPE_CHROME_EXTENSION" => Some(11),
        _ => None,
    }
}

/// 单个登录会话。已知字段单独抽出，`raw` 保留服务端原始对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSession {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub is_current: bool,
    pub raw: Value,
}

/// 会话列表
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSessionList {
    pub sessions: Vec<CursorSession>,
}

/// 按候选键依次取第一个非空字符串（数字也接受，转成字符串）
fn pick_str(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        match map.get(*key) {
            Some(Value::String(s)) if !s.trim().is_empty() => return Some(s.clone()),
            Some(Value::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

fn pick_bool(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_bool))
}

/// 地点可能是整串，也可能拆成 city / region / country
fn pick_location(map: &serde_json::Map<String, Value>) -> Option<String> {
    if let Some(direct) = pick_str(map, &["location", "geo_location", "geoLocation", "place"]) {
        return Some(direct);
    }
    let parts: Vec<String> = ["city", "region", "state", "country"]
        .iter()
        .filter_map(|key| pick_str(map, &[key]))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// 设备描述优先取可读名，退回 User-Agent
fn pick_device(map: &serde_json::Map<String, Value>) -> Option<String> {
    pick_str(
        map,
        &[
            "device_name",
            "deviceName",
            "device",
            "client_name",
            "clientName",
            "os",
            "browser",
            "user_agent",
            "userAgent",
        ],
    )
}

/// 把服务端返回的一条会话规整成 `CursorSession`；拿不到 id 的条目直接丢弃（无法撤销）
fn normalize_session(value: &Value, current_session_id: Option<&str>) -> Option<CursorSession> {
    let map = value.as_object()?;

    let id = pick_str(map, &["id", "session_id", "sessionId"])?;
    let session_type = pick_str(map, &["type", "session_type", "sessionType"]);

    // 服务端没给 isCurrent 时，退回用本地 session token 里的会话 id 认领当前设备
    let is_current = pick_bool(map, &["is_current", "isCurrent", "current"]).unwrap_or_else(|| {
        current_session_id.is_some_and(|current| current == id)
    });

    Some(CursorSession {
        id,
        session_type,
        ip_address: pick_str(map, &["ip_address", "ipAddress", "ip"]),
        location: pick_location(map),
        device: pick_device(map),
        last_active_at: pick_str(
            map,
            &[
                "last_active_at",
                "lastActiveAt",
                "last_seen_at",
                "lastSeenAt",
                "updated_at",
                "updatedAt",
            ],
        ),
        created_at: pick_str(map, &["created_at", "createdAt"]),
        is_current,
        raw: value.clone(),
    })
}

/// 从 `WorkosCursorSessionToken` 的 JWT 里读会话 id。
///
/// Cursor 签的 token 把它放在 `workosSessionId`（实测 payload：`sub`、`time`、
/// `randomness`、`exp`、`iss`、`scope`、`aud`、`type`、`workosSessionId`），
/// 值形如 `session_01XXXX`，和 `/api/auth/sessions` 列表里的 id 同一套编号。
/// `sid` 是 WorkOS 原生 token 的写法，留着兜底。
/// 只用于标记「当前设备」，取不到就算了。
fn session_id_from_token(session_token: &str) -> Option<String> {
    let jwt = if session_token.contains("%3A%3A") {
        session_token.split("%3A%3A").nth(1)?
    } else if session_token.contains("::") {
        session_token.split("::").nth(1)?
    } else {
        session_token
    };

    let payload = URL_SAFE_NO_PAD.decode(jwt.split('.').nth(1)?).ok()?;
    let json: Value = serde_json::from_slice(&payload).ok()?;
    ["workosSessionId", "workos_session_id", "sid", "session_id"]
        .iter()
        .find_map(|key| json.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

/// 未登录时 cursor.com 会 307 跳到 WorkOS 登录页，跟着跳完拿到的是 HTML。
/// 这种情况要报「会话失效」，而不是抛一句看不懂的 JSON 解析错误。
fn redirected_to_login(final_url: &reqwest::Url) -> bool {
    final_url
        .host_str()
        .is_none_or(|host| host != "cursor.com" && !host.ends_with(".cursor.com"))
}

/// 从 `{ "error": { "message": ... } }` 里取可读错误
fn error_message(body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    let error = json.get("error")?;
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(str::to_string)
}

/// 拉取当前账号的登录设备列表
pub async fn list_sessions(session_token: &str) -> Result<CursorSessionList, String> {
    let client = create_proxy_client()?;

    let response = client
        .get(SESSIONS_URL)
        .header(
            "Cookie",
            format!("WorkosCursorSessionToken={}", session_token),
        )
        .header("Accept", "application/json, text/plain, */*")
        .header("Referer", "https://cursor.com/dashboard/settings")
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("List sessions request failed: {}", e))?;

    let status = response.status();
    let final_url = response.url().clone();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read sessions response: {}", e))?;

    if redirected_to_login(&final_url) {
        return Err("Session expired: cursor.com redirected to login".to_string());
    }

    if !status.is_success() {
        return Err(match error_message(&body) {
            Some(message) => format!("Failed to list sessions (HTTP {}): {}", status, message),
            None => format!("Failed to list sessions (HTTP {})", status),
        });
    }

    let json: Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse sessions response: {}", e))?;
    if let Some(message) = error_message(&body) {
        return Err(message);
    }

    let current_session_id = session_id_from_token(session_token);
    let sessions = json
        .get("sessions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_session(item, current_session_id.as_deref()))
                .collect()
        })
        .unwrap_or_default();

    Ok(CursorSessionList { sessions })
}

/// 撤销一个登录会话。`session_type` 传列表里拿到的字符串枚举，未知类型会省略 `type`
pub async fn revoke_session(
    session_token: &str,
    session_id: &str,
    session_type: Option<&str>,
) -> Result<(), String> {
    let client = create_proxy_client()?;

    let mut payload = serde_json::json!({ "session_id": session_id });
    if let Some(value) = session_type.and_then(revoke_type_value) {
        payload["type"] = Value::from(value);
    }

    let response = client
        .post(REVOKE_URL)
        .header(
            "Cookie",
            format!("WorkosCursorSessionToken={}", session_token),
        )
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        // POST 走 CSRF 校验，缺 Origin 会被拒
        .header("Origin", "https://cursor.com")
        .header("Referer", "https://cursor.com/dashboard/settings")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("Revoke session request failed: {}", e))?;

    let status = response.status();
    let final_url = response.url().clone();
    let body = response.text().await.unwrap_or_default();

    if redirected_to_login(&final_url) {
        return Err("Session expired: cursor.com redirected to login".to_string());
    }

    if !status.is_success() {
        return Err(match error_message(&body) {
            Some(message) => format!("Failed to revoke session (HTTP {}): {}", status, message),
            None => format!("Failed to revoke session (HTTP {})", status),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_session_types_to_revoke_enum() {
        assert_eq!(revoke_type_value("SESSION_TYPE_WEB"), Some(1));
        assert_eq!(revoke_type_value("SESSION_TYPE_CLIENT"), Some(2));
        assert_eq!(revoke_type_value("SESSION_TYPE_MOBILE"), Some(10));
        assert_eq!(revoke_type_value("SESSION_TYPE_CHROME_EXTENSION"), Some(11));
        assert_eq!(revoke_type_value("SESSION_TYPE_FUTURE_THING"), None);
    }

    #[test]
    fn normalizes_snake_case_session() {
        let raw = serde_json::json!({
            "session_id": "session_01ABC",
            "type": "SESSION_TYPE_CLIENT",
            "ip_address": "203.0.113.7",
            "city": "Tokyo",
            "country": "JP",
            "user_agent": "Cursor/1.2.3",
            "last_active_at": "2026-08-24T10:00:00Z"
        });

        let session = normalize_session(&raw, None).expect("session parsed");
        assert_eq!(session.id, "session_01ABC");
        assert_eq!(session.session_type.as_deref(), Some("SESSION_TYPE_CLIENT"));
        assert_eq!(session.ip_address.as_deref(), Some("203.0.113.7"));
        assert_eq!(session.location.as_deref(), Some("Tokyo, JP"));
        assert_eq!(session.device.as_deref(), Some("Cursor/1.2.3"));
        assert!(!session.is_current);
    }

    #[test]
    fn normalizes_camel_case_session() {
        let raw = serde_json::json!({
            "id": "session_01XYZ",
            "sessionType": "SESSION_TYPE_WEB",
            "ipAddress": "198.51.100.4",
            "location": "Berlin, DE",
            "deviceName": "Chrome on macOS",
            "lastActiveAt": "2026-08-25T06:00:00Z",
            "isCurrent": true
        });

        let session = normalize_session(&raw, None).expect("session parsed");
        assert_eq!(session.id, "session_01XYZ");
        assert_eq!(session.session_type.as_deref(), Some("SESSION_TYPE_WEB"));
        assert_eq!(session.device.as_deref(), Some("Chrome on macOS"));
        assert!(session.is_current);
    }

    #[test]
    fn falls_back_to_token_session_id_for_current_device() {
        let raw = serde_json::json!({ "id": "session_01ABC", "type": "SESSION_TYPE_WEB" });

        assert!(normalize_session(&raw, Some("session_01ABC")).unwrap().is_current);
        assert!(!normalize_session(&raw, Some("session_01OTHER")).unwrap().is_current);
    }

    #[test]
    fn drops_entries_without_an_id() {
        // 没有 id 就没法撤销，留在列表里只会点了报错
        let raw = serde_json::json!({ "type": "SESSION_TYPE_WEB", "ip_address": "203.0.113.7" });
        assert!(normalize_session(&raw, None).is_none());
    }

    #[test]
    fn keeps_unknown_fields_in_raw() {
        let raw = serde_json::json!({ "id": "s1", "somethingNew": "keep me" });
        let session = normalize_session(&raw, None).unwrap();
        assert_eq!(session.raw["somethingNew"], "keep me");
    }

    /// 拼一个 `user_xxx::<jwt>` 形态的 session token，payload 为给定 JSON
    fn session_token_with_payload(payload: &str, separator: &str) -> String {
        format!(
            "user_01{}eyJhbGciOiJIUzI1NiJ9.{}.sig",
            separator,
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        )
    }

    #[test]
    fn reads_workos_session_id_from_token() {
        // Cursor 实际签发的 payload 形态：会话 id 在 workosSessionId，没有 sid
        let token = session_token_with_payload(
            r#"{"sub":"grok|user_01","type":"web","workosSessionId":"session_01ABC"}"#,
            "%3A%3A",
        );
        assert_eq!(
            session_id_from_token(&token).as_deref(),
            Some("session_01ABC")
        );
    }

    #[test]
    fn reads_session_id_from_legacy_keys() {
        for key in ["workos_session_id", "sid", "session_id"] {
            let token =
                session_token_with_payload(&format!(r#"{{"{key}":"session_01ABC"}}"#), "::");
            assert_eq!(
                session_id_from_token(&token).as_deref(),
                Some("session_01ABC"),
                "failed for key {key}"
            );
        }
    }

    #[test]
    fn prefers_workos_session_id_over_sid() {
        let token = session_token_with_payload(
            r#"{"sid":"session_01LEGACY","workosSessionId":"session_01ABC"}"#,
            "::",
        );
        assert_eq!(
            session_id_from_token(&token).as_deref(),
            Some("session_01ABC")
        );
    }

    #[test]
    fn returns_none_when_token_has_no_session_id() {
        let token = session_token_with_payload(r#"{"sub":"grok|user_01"}"#, "::");
        assert_eq!(session_id_from_token(&token), None);
        assert_eq!(session_id_from_token("not-a-jwt"), None);
    }

    #[test]
    fn serializes_the_keys_the_modal_reads() {
        let raw = serde_json::json!({
            "id": "s1",
            "type": "SESSION_TYPE_CLIENT",
            "ip_address": "203.0.113.7",
            "location": "Tokyo, JP",
            "device_name": "Cursor on macOS",
            "last_active_at": "2026-08-25T06:00:00Z",
            "created_at": "2026-08-01T06:00:00Z"
        });
        let list = CursorSessionList {
            sessions: vec![normalize_session(&raw, None).unwrap()],
        };

        let json = serde_json::to_value(&list).unwrap();
        let session = &json["sessions"][0];
        for key in [
            "id",
            "sessionType",
            "ipAddress",
            "location",
            "device",
            "lastActiveAt",
            "createdAt",
            "isCurrent",
            "raw",
        ] {
            assert!(session.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn extracts_error_message() {
        let body = r#"{"error":{"message":"not_authenticated"}}"#;
        assert_eq!(error_message(body).as_deref(), Some("not_authenticated"));
        assert_eq!(error_message(r#"{"sessions":[]}"#), None);
    }
}
