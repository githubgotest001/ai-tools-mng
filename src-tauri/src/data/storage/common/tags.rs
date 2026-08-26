//! 账号标签（多标签，最多 MAX_ACCOUNT_TAGS 个）。
//!
//! 每个平台的 Account 同时保留两套字段：
//! - `tags`：真正的标签列表，存 Postgres 的 `tags JSONB` 列；
//! - `tag` / `tag_color`：第一个标签的镜像，旧列保持原样。
//!
//! 镜像列不能删。账号会整体经由 Postgres 同步和 WebDAV 备份流到别的客户端，
//! 旧版本只认 `tag` / `tag_color`；反过来读到只有旧字段的账号时，也要能当成
//! 单个标签渲染出来。规范化（去重、截断、镜像同步）由前端 accountTags.js
//! 统一负责，这里只做类型承载与读取兜底。

use serde::{Deserialize, Serialize};

/// 单个账号最多挂几个标签
pub const MAX_ACCOUNT_TAGS: usize = 3;

/// 一个标签
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTag {
    pub name: String,
    /// 十六进制颜色；缺省时由前端 accountTags.js 补默认色
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// 从 `tags` 列反序列化，顺带丢掉空名字并截断到上限。
///
/// 手工改库或旧客户端写进来的脏数据不该让整个账号解析失败，
/// 所以这里对非数组、非法元素一律降级成空列表而不是报错。
pub fn tags_from_json(value: Option<serde_json::Value>) -> Vec<AccountTag> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(tags) = serde_json::from_value::<Vec<AccountTag>>(value) else {
        return Vec::new();
    };
    normalize_tags(tags)
}

/// 去重（不区分大小写）并截断到 MAX_ACCOUNT_TAGS
pub fn normalize_tags(tags: Vec<AccountTag>) -> Vec<AccountTag> {
    let mut seen: Vec<String> = Vec::new();
    let mut result = Vec::new();
    for tag in tags {
        let name = tag.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        result.push(AccountTag {
            name,
            color: tag.color,
        });
        if result.len() >= MAX_ACCOUNT_TAGS {
            break;
        }
    }
    result
}

/// 序列化成 `tags` 列的值；空列表写 NULL，避免库里堆一堆 `[]`
pub fn tags_to_json(tags: &[AccountTag]) -> Option<serde_json::Value> {
    if tags.is_empty() {
        return None;
    }
    serde_json::to_value(tags).ok()
}

/// 读取时的兜底：`tags` 为空就把旧的 `tag` / `tag_color` 当成单个标签。
///
/// 存量账号（升级前打的标签只写进了旧列）靠这条才不会显示成「无标签」。
pub fn tags_or_legacy(
    tags: Vec<AccountTag>,
    tag: Option<&str>,
    tag_color: Option<&str>,
) -> Vec<AccountTag> {
    if !tags.is_empty() {
        return tags;
    }
    match tag.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => vec![AccountTag {
            name: name.to_string(),
            color: tag_color
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string),
        }],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, color: Option<&str>) -> AccountTag {
        AccountTag {
            name: name.to_string(),
            color: color.map(str::to_string),
        }
    }

    #[test]
    fn truncates_beyond_the_limit() {
        let tags = normalize_tags(vec![
            tag("a", None),
            tag("b", None),
            tag("c", None),
            tag("d", None),
        ]);
        assert_eq!(tags.len(), MAX_ACCOUNT_TAGS);
        assert_eq!(tags[2].name, "c");
    }

    #[test]
    fn dedupes_case_insensitively_keeping_the_first_color() {
        let tags = normalize_tags(vec![tag("Work", Some("#111111")), tag("work", Some("#222222"))]);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Work");
        assert_eq!(tags[0].color.as_deref(), Some("#111111"));
    }

    #[test]
    fn drops_blank_names_and_trims() {
        let tags = normalize_tags(vec![tag("   ", None), tag("  spaced  ", None)]);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "spaced");
    }

    #[test]
    fn malformed_column_degrades_to_empty_instead_of_failing() {
        assert!(tags_from_json(None).is_empty());
        assert!(tags_from_json(Some(serde_json::json!("oops"))).is_empty());
        assert!(tags_from_json(Some(serde_json::json!([{ "colour": "#fff" }]))).is_empty());

        let tags = tags_from_json(Some(serde_json::json!([
            { "name": "Team", "color": "#abcdef" },
            { "name": "Paid" }
        ])));
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].color.as_deref(), Some("#abcdef"));
        assert_eq!(tags[1].color, None);
    }

    #[test]
    fn empty_list_is_written_as_null() {
        assert!(tags_to_json(&[]).is_none());
        assert!(tags_to_json(&[tag("x", None)]).is_some());
    }

    #[test]
    fn falls_back_to_the_legacy_single_tag_columns() {
        // 升级前打的标签只在旧列里，必须能读出来
        let tags = tags_or_legacy(Vec::new(), Some("自用"), Some("#3b82f6"));
        assert_eq!(tags, vec![tag("自用", Some("#3b82f6"))]);

        // 新列有值就完全忽略旧列，不做合并
        let tags = tags_or_legacy(vec![tag("New", None)], Some("Old"), None);
        assert_eq!(tags, vec![tag("New", None)]);

        assert!(tags_or_legacy(Vec::new(), Some("  "), None).is_empty());
        assert!(tags_or_legacy(Vec::new(), None, Some("#fff")).is_empty());
    }
}
