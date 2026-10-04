//! Cursor 登录 deeplink（无感切换）
//!
//! Cursor 渲染进程注册了 authority 为 `cursorAuth` 的 URL 处理器（handleAuth），
//! `route=login` 且带 accessToken / refreshToken 时直接 storeAccessRefreshToken + refreshMembership，
//! 无需重启：`cursor://cursorAuth/?route=login&accessToken=..&refreshToken=..`
//! 这里负责：拼 URL、定位安装目录、读版本号、检测当前安装是否带该处理器。
//!
//! 注意：URL 里带 token，绝不能打印或写进错误信息。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// 构造登录 deeplink
pub fn build_login_url(access_token: &str, refresh_token: &str) -> String {
    format!(
        "cursor://cursorAuth/?route=login&accessToken={}&refreshToken={}",
        urlencoding::encode(access_token),
        urlencoding::encode(refresh_token)
    )
}

/// 由 Cursor 路径推出 `resources/app` 目录
/// - Windows：`<安装目录>\Cursor.exe` -> `<安装目录>\resources\app`
/// - macOS：`Cursor.app` 或 `Cursor.app/Contents/MacOS/Cursor` -> `Cursor.app/Contents/Resources/app`
/// - Linux：`/usr/share/cursor/cursor`（/usr/bin/cursor 软链会先解析）-> `/usr/share/cursor/resources/app`
pub fn app_resources_dir(cursor_path: &Path) -> Option<PathBuf> {
    let path = std::fs::canonicalize(cursor_path).unwrap_or_else(|_| cursor_path.to_path_buf());

    let mut candidates: Vec<PathBuf> = Vec::new();
    let s = path.to_string_lossy().to_string();
    if let Some(idx) = s.find(".app") {
        let bundle_end = idx + 4;
        if s.len() == bundle_end || s[bundle_end..].starts_with('/') {
            let bundle = PathBuf::from(&s[..bundle_end]);
            candidates.push(bundle.join("Contents").join("Resources").join("app"));
        }
    }
    if path.is_dir() {
        candidates.push(path.join("resources").join("app"));
    }
    if let Some(parent) = path.parent() {
        candidates.push(parent.join("resources").join("app"));
    }

    candidates
        .into_iter()
        .find(|dir| dir.join("product.json").is_file() || dir.join("package.json").is_file())
}

/// 读取 Cursor 版本号（product.json 优先，package.json 兜底）
pub fn cursor_version(cursor_path: &Path) -> Option<String> {
    let app_dir = app_resources_dir(cursor_path)?;
    for file in ["product.json", "package.json"] {
        let Ok(content) = std::fs::read_to_string(app_dir.join(file)) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if let Some(v) = json.get("version").and_then(|v| v.as_str()) {
            return Some(v.to_string());
        }
    }
    None
}

/// 检测结果缓存：key = workbench 文件路径，value = (mtime, 是否支持)
fn support_cache() -> &'static Mutex<HashMap<PathBuf, (Option<SystemTime>, bool)>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Option<SystemTime>, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn has_login_handler(content: &str) -> bool {
    let has_authority = content.contains("authority===\"cursorAuth\"")
        || content.contains("authority==='cursorAuth'")
        || content.contains("authority === \"cursorAuth\"");
    if !has_authority {
        return false;
    }
    // 处理器里会同时取 refreshToken 和 accessToken 两个 query 参数
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r#"get\(\s*["']refreshToken["']\s*\)\s*,\s*[\w$]+\s*=\s*[\w$]+\.get\(\s*["']accessToken["']\s*\)|get\(\s*["']accessToken["']\s*\)\s*,\s*[\w$]+\s*=\s*[\w$]+\.get\(\s*["']refreshToken["']\s*\)"#,
        )
        .expect("valid regex")
    });
    re.is_match(content)
}

/// 当前安装的 Cursor 是否带 `cursorAuth` 登录 deeplink 处理器（已在 3.23.x 上验证）
pub fn supports_auth_deeplink(cursor_path: &Path) -> Result<bool, String> {
    let app_dir = app_resources_dir(cursor_path)
        .ok_or_else(|| format!("Cannot locate Cursor resources from {}", cursor_path.display()))?;

    let workbench_dir = app_dir.join("out").join("vs").join("workbench");
    let candidates = [
        workbench_dir.join("workbench.desktop.main.js"),
        workbench_dir.join("workbench.glass.main.js"),
    ];

    let mut found_any = false;
    for file in candidates.iter().filter(|f| f.is_file()) {
        found_any = true;
        let mtime = std::fs::metadata(file).and_then(|m| m.modified()).ok();

        if let Ok(cache) = support_cache().lock() {
            if let Some((cached_mtime, supported)) = cache.get(file) {
                if *cached_mtime == mtime {
                    if *supported {
                        return Ok(true);
                    }
                    continue;
                }
            }
        }

        // workbench 体积约 40MB，只在版本变化（mtime 变）后才重新扫描
        let bytes = std::fs::read(file)
            .map_err(|e| format!("Failed to read {}: {}", file.display(), e))?;
        let content = String::from_utf8_lossy(&bytes);
        let supported = has_login_handler(&content);

        if let Ok(mut cache) = support_cache().lock() {
            cache.insert(file.clone(), (mtime, supported));
        }
        if supported {
            return Ok(true);
        }
    }

    if !found_any {
        return Err(format!(
            "Cursor workbench bundle not found under {}",
            workbench_dir.display()
        ));
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_is_encoded() {
        let url = build_login_url("a.b+c/d=", "e&f");
        assert!(url.starts_with("cursor://cursorAuth/?route=login&accessToken="));
        assert!(url.contains("accessToken=a.b%2Bc%2Fd%3D"));
        assert!(url.ends_with("refreshToken=e%26f"));
    }

    #[test]
    fn detects_handler_in_minified_bundle() {
        let sample = r#"if(($.scheme==="cursor")&&$.authority==="cursorAuth"){const Y=new URLSearchParams($.query);return this.handleAuth(Y),!0}handleAuth(e){const t=e.get("route");switch(t){case"login":{const n=e.get("refreshToken"),i=e.get("accessToken");}}}"#;
        assert!(has_login_handler(sample));
        assert!(!has_login_handler(r#"e.authority==="cursorAuth";x.get("foo")"#));
        assert!(!has_login_handler(r#"t.get("refreshToken"),n=t.get("accessToken")"#));
    }

    #[test]
    fn resolves_resources_dir_next_to_executable() {
        let tmp = std::env::temp_dir().join(format!("atm-deeplink-test-{}", std::process::id()));
        let app = tmp.join("resources").join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("product.json"), r#"{"version":"3.23.12"}"#).unwrap();
        let exe = tmp.join("Cursor.exe");
        std::fs::write(&exe, b"").unwrap();

        assert_eq!(cursor_version(&exe).as_deref(), Some("3.23.12"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
