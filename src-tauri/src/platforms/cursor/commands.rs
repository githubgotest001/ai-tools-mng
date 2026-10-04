//! Cursor Tauri Commands

use crate::cursor::models::{Account, MachineInfo, TokenData};
use crate::cursor::modules::{auth, db, deeplink, machine, process, sessions, storage};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// 从 JWT token 中解析 exp 字段（过期时间戳）
fn parse_jwt_exp(token: &str) -> Option<i64> {
    // JWT 格式: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // 解码 payload (第二部分)
    let payload = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let payload_str = String::from_utf8(payload).ok()?;

    // 解析 JSON 并提取 exp 字段
    let json: serde_json::Value = serde_json::from_str(&payload_str).ok()?;
    json.get("exp")?.as_i64()
}

/// 从 session token 中解析 exp 字段
/// session token 格式: user_id%3A%3A<JWT> 或 user_id::<JWT>
fn parse_session_token_exp(session_token: &str) -> Option<i64> {
    // 提取 JWT 部分
    let jwt = if session_token.contains("%3A%3A") {
        session_token.split("%3A%3A").nth(1)?
    } else if session_token.contains("::") {
        session_token.split("::").nth(1)?
    } else {
        return None;
    };

    parse_jwt_exp(jwt)
}

/// 账号列表响应
#[derive(Serialize)]
pub struct AccountListResponse {
    pub accounts: Vec<Account>,
    pub current_account_id: Option<String>,
}

/// 列出所有账号
#[tauri::command]
pub async fn cursor_list_accounts(app: AppHandle) -> Result<AccountListResponse, String> {
    let accounts = storage::list_accounts(&app).await?;
    let current_account_id = storage::get_current_account_id(&app).await?;

    Ok(AccountListResponse {
        accounts,
        current_account_id,
    })
}

/// 删除账号
#[tauri::command]
pub async fn cursor_delete_account(app: AppHandle, account_id: String) -> Result<(), String> {
    let deleted = storage::delete_account(&app, &account_id).await?;
    if !deleted {
        return Err(format!("Account not found: {}", account_id));
    }
    Ok(())
}

/// 切换账号响应
#[derive(Serialize)]
pub struct SwitchAccountResponse {
    pub message: String,
    /// 非致命问题（例如机器码 / main.js 修改失败），切号本身已完成
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// 无感切号响应
#[derive(Serialize)]
pub struct SeamlessSwitchAccountResponse {
    pub success: bool,
    pub message: String,
    /// deeplink：Cursor 运行中、不重启完成切换；direct：Cursor 未运行，直接写库并启动；
    /// restart：deeplink 不可用或未生效，回退到「关闭 → 写库 → 启动」
    pub mode: &'static str,
    pub fallback_used: bool,
    pub fallback_reason: Option<String>,
    pub cursor_version: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
struct SwitchProgressPayload {
    step: &'static str,
    label: String,
    percent: u8,
    phase: &'static str,
}

fn emit_switch_progress(
    app: &AppHandle,
    step: &'static str,
    label: impl Into<String>,
    percent: u8,
    phase: &'static str,
) {
    let payload = SwitchProgressPayload {
        step,
        label: label.into(),
        percent,
        phase,
    };
    let _ = app.emit("cursor-switch-progress", payload);
}

/// 同一时刻只允许一个切号流程（重复点击 / 普通与无感同时触发会互相踩数据库）
fn switch_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// access token 剩余有效期低于该值时，切号前先刷新
const TOKEN_REFRESH_MARGIN_SECS: i64 = 24 * 3600;

/// 切号前确保账号 token 可用：快过期就刷新，并用 Stripe 接口实测一次。
///
/// 必须在关闭 Cursor 之前调用——旧逻辑先关 Cursor 再注入一个早已过期的 token，
/// Cursor 启动后直接掉回未登录，表现就是「切号失败」。
/// 返回非致命警告（例如网络不通无法验证）。token 本身永远不会出现在日志或错误里。
async fn ensure_fresh_token(app: &AppHandle, acc: &mut Account) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    let now = chrono::Utc::now().timestamp();

    if acc.token.access_token.trim().is_empty() {
        return Err("账号没有 access token，请重新添加或刷新该账号".to_string());
    }

    let exp = parse_jwt_exp(&acc.token.access_token).unwrap_or(acc.token.expiry_timestamp);
    let mut refreshed = false;

    if exp > 0 && exp - now < TOKEN_REFRESH_MARGIN_SECS {
        println!(
            "[cursor-switch] access token expires in {}s, refreshing before switch",
            exp - now
        );
        match refresh_account_token(acc).await {
            Ok(()) => refreshed = true,
            Err(e) if exp > now + 60 => {
                // 还没真正过期，先用着
                warnings.push(format!("Token 即将过期且刷新失败：{}", e));
            }
            Err(e) => {
                return Err(format!("账号 token 已过期且刷新失败：{}", e));
            }
        }
    }

    // 实测 token：401/403 说明 token 已被吊销（例如在网页上登出），再尝试刷新一次
    match auth::get_stripe_profile(&acc.token.access_token).await {
        // 这里只验证 token，不改套餐：套餐以「刷新配额」(usage-summary) 为准，
        // 切号时用 Stripe 字段覆盖会让 pro_student 在切号后变成 pro、刷新后又变回去
        Ok(_) => {}
        Err(e) if is_auth_rejected(&e) => {
            if refreshed {
                return Err("刷新后的 token 仍被 Cursor 拒绝，请重新登录该账号".to_string());
            }
            println!("[cursor-switch] token rejected by server, trying one refresh");
            refresh_account_token(acc)
                .await
                .map_err(|e| format!("账号 token 已失效且刷新失败：{}", e))?;
            refreshed = true;
            if let Err(e) = auth::get_stripe_profile(&acc.token.access_token).await {
                if is_auth_rejected(&e) {
                    return Err("刷新后的 token 仍被 Cursor 拒绝，请重新登录该账号".to_string());
                }
                warnings.push("无法验证 token 有效性（网络异常），继续切换".to_string());
            }
        }
        Err(_) => {
            warnings.push("无法验证 token 有效性（网络异常），继续切换".to_string());
        }
    }

    if refreshed {
        acc.updated_at = chrono::Utc::now().timestamp();
        // 以存储里的最新账号为基础只改 token 相关字段，避免覆盖并发写入的用量 / 套餐数据
        let mut latest = storage::load_account(app, &acc.id)
            .await
            .unwrap_or_else(|_| acc.clone());
        merge_refreshed_token(&mut latest, acc);
        storage::save_account(app, &latest).await?;
        *acc = latest;
    }

    Ok(warnings)
}

/// 把切号前刷新得到的 token 合并进存储里的最新账号。
/// 只动 token / session 失效标记 / 更新时间；套餐、用量、标签等一律保留存储里的值。
fn merge_refreshed_token(latest: &mut Account, refreshed: &Account) {
    latest.token = refreshed.token.clone();
    latest.session_invalid_at = refreshed.session_invalid_at;
    latest.updated_at = refreshed.updated_at;
}

fn is_auth_rejected(err: &str) -> bool {
    err.contains("HTTP 401") || err.contains("HTTP 403")
}

/// 刷新账号 token：有 session 先走 PKCE（拿到的是全新长期 token），失败再用 refresh_token
async fn refresh_account_token(acc: &mut Account) -> Result<(), String> {
    let mut last_err = String::from("没有可用的 session token 或 refresh token");

    if let Some(session) = acc.token.workos_cursor_session_token.clone() {
        match auth::get_access_token_from_session(&session).await {
            Ok(resp) => {
                apply_token_response(acc, resp);
                acc.session_invalid_at = None;
                return Ok(());
            }
            Err(e) => {
                println!("[cursor-switch] session based refresh failed: {}", e);
                last_err = e;
            }
        }
    }

    if !acc.token.refresh_token.trim().is_empty() {
        match auth::refresh_access_token(&acc.token.refresh_token).await {
            Ok(resp) => {
                apply_token_response(acc, resp);
                return Ok(());
            }
            Err(e) => {
                println!("[cursor-switch] refresh_token based refresh failed: {}", e);
                last_err = e;
            }
        }
    }

    Err(last_err)
}

fn apply_token_response(acc: &mut Account, resp: auth::AccessTokenResponse) {
    if let Some(exp) = parse_jwt_exp(&resp.access_token) {
        acc.token.expiry_timestamp = exp;
    }
    acc.token.refresh_token = resp
        .refresh_token
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| resp.access_token.clone());
    acc.token.access_token = resp.access_token;
}

/// 重启切号时的机器码策略
#[derive(Clone, Copy, PartialEq, Eq)]
enum MachineMode {
    /// 不动机器码（无感切换回退时使用，保持与 deeplink 模式行为一致）
    Skip,
    /// 随机生成新机器码
    Random,
    /// 使用账号绑定的机器码
    Bound,
}

/// 机器码 + main.js 处理。全部失败都只记警告：这一步不影响登录态本身，
/// 旧版本把它当致命错误，main.js 在 Program Files 下没写权限时整个切号直接失败
fn apply_machine_ids(
    mode: MachineMode,
    machine_info: Option<MachineInfo>,
    cursor_path: Option<String>,
) -> Vec<String> {
    let mut warnings = Vec::new();

    let ids_result = match (mode, machine_info) {
        (MachineMode::Skip, _) => return warnings,
        (MachineMode::Bound, Some(info)) => machine::write_machine_ids(&info),
        _ => machine::reset_machine_id().map(|_| ()),
    };
    if let Err(e) = ids_result {
        eprintln!("[cursor-switch] machine id update failed: {}", e);
        warnings.push(format!("机器码更新失败（已跳过）：{}", e));
    }

    if let Err(e) = machine::modify_main_js(cursor_path.as_deref()) {
        eprintln!("[cursor-switch] main.js patch failed: {}", e);
        warnings.push(format!(
            "main.js 修改失败（已跳过，不影响登录）：{}。如 Cursor 安装在 Program Files，请以管理员身份运行本工具",
            e
        ));
    }

    warnings
}

/// 把 Cursor 的认证相关键备份成 JSON（替代原来整库复制 state.vscdb.backup）
fn backup_auth_state(app: &AppHandle, db_path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
        .join("cursor_auth_backups");
    db::export_auth_backup(db_path, &dir, 10)
}

/// 重启式切号主流程（调用方负责加锁、加载账号、保证 token 可用）
async fn restart_switch_inner(
    app: &AppHandle,
    acc: &mut Account,
    mode: MachineMode,
    progress_base: u8,
) -> Result<Vec<String>, String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};

    let mut warnings = Vec::new();
    let span = 100u8.saturating_sub(progress_base).max(1) as u32;
    let pct = |p: u32| -> u8 { (progress_base as u32 + span * p / 100).min(99) as u8 };

    // 1. 关闭之前先记住 Cursor 的实际路径（自定义 > 正在运行 > 默认 / 注册表）
    let custom_path = read_custom_path_from_config(app, &CURSOR_CONFIG);
    let cp = custom_path.clone();
    let cursor_path = tokio::task::spawn_blocking(move || process::resolve_cursor_path(cp.as_deref()))
        .await
        .map_err(|e| format!("Resolve Cursor path task failed: {}", e))?;
    let cursor_path_str = cursor_path.as_ref().map(|p| p.to_string_lossy().to_string());

    // 2. 关闭 Cursor，并等待数据库写锁释放
    emit_switch_progress(app, "close", "正在关闭 Cursor...", pct(10), "running");
    tokio::task::spawn_blocking(|| process::close_cursor(15))
        .await
        .map_err(|e| format!("Close Cursor task failed: {}", e))??;

    let db_path = db::get_db_path()?;
    let dbp = db_path.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        db::wait_for_db_release(&dbp, std::time::Duration::from_secs(10))
    })
    .await
    .map_err(|e| format!("Wait database task failed: {}", e))?
    {
        // 写入时还有 busy_timeout 兜底，这里只提示
        eprintln!("[cursor-switch] {}", e);
        warnings.push(format!("Cursor 数据库仍被占用：{}", e));
    }

    // 3. 机器码（非致命）
    if mode != MachineMode::Skip {
        emit_switch_progress(app, "machine", "正在更新机器码...", pct(35), "running");
        let info = if mode == MachineMode::Bound {
            acc.machine_info.clone()
        } else {
            None
        };
        let cps = cursor_path_str.clone();
        let machine_warnings = tokio::task::spawn_blocking(move || apply_machine_ids(mode, info, cps))
            .await
            .map_err(|e| format!("Machine ID task failed: {}", e))?;
        warnings.extend(machine_warnings);
    }

    // 4. 备份 + 写入认证状态（致命）
    emit_switch_progress(app, "write", "正在写入登录信息...", pct(60), "running");
    match backup_auth_state(app, &db_path) {
        Ok(file) => println!("[cursor-switch] auth backup saved: {}", file.display()),
        Err(e) => eprintln!("[cursor-switch] auth backup skipped: {}", e),
    }
    let (dbp, at, rt, email) = (
        db_path.clone(),
        acc.token.access_token.clone(),
        acc.token.refresh_token.clone(),
        acc.email.clone(),
    );
    tokio::task::spawn_blocking(move || db::write_cursor_auth_state(&dbp, &at, &rt, &email))
        .await
        .map_err(|e| format!("Write auth task failed: {}", e))??;

    // 5. 更新当前账号与最后使用时间
    storage::set_current_account_id(app, Some(acc.id.clone())).await?;
    acc.update_last_used();
    storage::save_account(app, acc).await?;

    // 6. 启动 Cursor（启动失败不回滚：登录信息已写好，用户手动打开即可）
    emit_switch_progress(app, "launch", "正在启动 Cursor...", pct(85), "running");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let launch_result = process::launch_cursor_with_path(cursor_path_str.as_deref()).or_else(|e| {
        if cursor_path_str.is_some() {
            eprintln!("[cursor-switch] launch with resolved path failed, retrying default: {}", e);
            process::launch_cursor_with_path(custom_path.as_deref())
        } else {
            Err(e)
        }
    });
    if let Err(e) = launch_result {
        warnings.push(format!("账号已切换，但启动 Cursor 失败，请手动打开：{}", e));
    }

    Ok(warnings)
}

/// 切换账号（完整流程：关闭 Cursor → 机器码 → 写入登录信息 → 启动）
/// use_bound_machine_id: 是否使用账号绑定的机器码（如果有的话）
/// reset_machine_id: 是否处理机器码（默认 true；false 时只换登录信息）
#[tauri::command]
pub async fn cursor_switch_account(
    app: AppHandle,
    account_id: String,
    use_bound_machine_id: Option<bool>,
    reset_machine_id: Option<bool>,
) -> Result<SwitchAccountResponse, String> {
    let _guard = switch_lock()
        .try_lock()
        .map_err(|_| "已有切换操作正在进行，请稍候".to_string())?;

    // 1. 加载账号
    let mut acc = storage::load_account(&app, &account_id).await?;

    // 2. 关闭 Cursor 之前先确认 token 可用（过期就刷新），失败直接返回，不动用户当前的 Cursor
    emit_switch_progress(&app, "token", "正在检查账号 token...", 10, "running");
    let mut warnings = match ensure_fresh_token(&app, &mut acc).await {
        Ok(w) => w,
        Err(e) => {
            emit_switch_progress(&app, "done", "切换失败", 100, "error");
            return Err(e);
        }
    };

    let mode = if !reset_machine_id.unwrap_or(true) {
        MachineMode::Skip
    } else if use_bound_machine_id.unwrap_or(false) && acc.has_machine_info() {
        MachineMode::Bound
    } else {
        MachineMode::Random
    };

    match restart_switch_inner(&app, &mut acc, mode, 20).await {
        Ok(w) => warnings.extend(w),
        Err(e) => {
            emit_switch_progress(&app, "done", "切换失败", 100, "error");
            return Err(e);
        }
    }

    emit_switch_progress(&app, "done", "切换完成", 100, "success");
    Ok(SwitchAccountResponse {
        message: format!("Account switched and Cursor started: {}", acc.email),
        warnings,
    })
}

/// deeplink 投递后等待 Cursor 把新 token 落库的最长时间
const DEEPLINK_VERIFY_TIMEOUT_MS: u64 = 8000;

/// 无感切换：Cursor 运行中时通过 `cursor://cursorAuth` 登录 deeplink 直接换号，不重启；
/// 不支持或未生效时（allow_fallback 默认 true）自动回退到重启切号。
/// 无感模式不修改机器码。
#[tauri::command]
pub async fn cursor_switch_account_seamless(
    app: AppHandle,
    account_id: String,
    allow_fallback: Option<bool>,
) -> Result<SeamlessSwitchAccountResponse, String> {
    let _guard = switch_lock()
        .try_lock()
        .map_err(|_| "已有切换操作正在进行，请稍候".to_string())?;
    let allow_fallback = allow_fallback.unwrap_or(true);

    emit_switch_progress(&app, "preparing", "正在准备账号信息...", 5, "running");
    let mut acc = storage::load_account(&app, &account_id).await?;

    // 1. token 不可用时直接报错，不做任何回退（回退也只会写进一个无效 token）
    emit_switch_progress(&app, "token", "正在检查账号 token...", 15, "running");
    let mut warnings = match ensure_fresh_token(&app, &mut acc).await {
        Ok(w) => w,
        Err(e) => {
            emit_switch_progress(&app, "done", "切换失败", 100, "error");
            return Err(e);
        }
    };

    let result = seamless_switch_inner(&app, &mut acc, allow_fallback, &mut warnings).await;
    match result {
        Ok((mode, fallback_reason, cursor_version)) => {
            emit_switch_progress(&app, "done", "切换完成", 100, "success");
            let fallback_used = mode == "restart";
            let message = match mode {
                "deeplink" => format!("Account switched without restarting Cursor: {}", acc.email),
                "direct" => format!("Account switched and Cursor started: {}", acc.email),
                _ => format!("Account switched by restarting Cursor: {}", acc.email),
            };
            Ok(SeamlessSwitchAccountResponse {
                success: true,
                message,
                mode,
                fallback_used,
                fallback_reason,
                cursor_version,
                warnings,
            })
        }
        Err(e) => {
            emit_switch_progress(&app, "done", "切换失败", 100, "error");
            Err(e)
        }
    }
}

/// 返回 (mode, fallback_reason, cursor_version)
async fn seamless_switch_inner(
    app: &AppHandle,
    acc: &mut Account,
    allow_fallback: bool,
    warnings: &mut Vec<String>,
) -> Result<(&'static str, Option<String>, Option<String>), String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};

    let db_path = db::get_db_path()?;
    let custom_path = read_custom_path_from_config(app, &CURSOR_CONFIG);

    // 2. Cursor 没在运行：不需要 deeplink，直接写库再启动（不动机器码）
    if !process::is_cursor_running() {
        emit_switch_progress(app, "write", "Cursor 未运行，直接写入登录信息...", 50, "running");
        let w = restart_switch_inner(app, acc, MachineMode::Skip, 50).await?;
        warnings.extend(w);
        let version = process::resolve_cursor_path(custom_path.as_deref())
            .and_then(|p| deeplink::cursor_version(&p));
        return Ok(("direct", None, version));
    }

    // 3. 检测当前安装是否支持登录 deeplink
    emit_switch_progress(app, "detect", "正在检测 Cursor 版本...", 30, "running");
    let cp = custom_path.clone();
    let (cursor_path, version, support) = tokio::task::spawn_blocking(move || {
        let path = process::resolve_cursor_path(cp.as_deref());
        let version = path.as_deref().and_then(deeplink::cursor_version);
        let support = match path.as_deref() {
            Some(p) => deeplink::supports_auth_deeplink(p),
            None => Err("未找到 Cursor 安装路径".to_string()),
        };
        (path, version, support)
    })
    .await
    .map_err(|e| format!("Detect Cursor task failed: {}", e))?;

    let deeplink_result: Result<(), String> = match support {
        Ok(true) => {
            try_deeplink_login(app, acc, cursor_path.as_deref(), &db_path).await
        }
        Ok(false) => Err(format!(
            "当前 Cursor 版本{}不支持登录 deeplink",
            version.as_deref().map(|v| format!(" {} ", v)).unwrap_or_default()
        )),
        Err(e) => Err(format!("无法检测 Cursor 是否支持无感切换：{}", e)),
    };

    match deeplink_result {
        Ok(()) => {
            storage::set_current_account_id(app, Some(acc.id.clone())).await?;
            acc.update_last_used();
            storage::save_account(app, acc).await?;

            // deeplink 不会更新 cachedEmail，补写一下（失败无所谓，重启后 Cursor 会自己刷新）
            let (dbp, email) = (db_path.clone(), acc.email.clone());
            if let Ok(Err(e)) =
                tokio::task::spawn_blocking(move || db::write_cached_email(&dbp, &email)).await
            {
                eprintln!("[cursor-switch] update cachedEmail skipped: {}", e);
            }
            Ok(("deeplink", None, version))
        }
        Err(reason) => {
            eprintln!("[cursor-switch] seamless switch unavailable: {}", reason);
            if !allow_fallback {
                return Err(format!("无感切换失败：{}", reason));
            }
            emit_switch_progress(app, "fallback", "无感切换未生效，回退为重启切换...", 55, "running");
            let w = restart_switch_inner(app, acc, MachineMode::Skip, 55).await?;
            warnings.extend(w);
            Ok(("restart", Some(reason), version))
        }
    }
}

/// 投递 deeplink 并只读轮询 state.vscdb，确认 Cursor 已经存下了目标 token
async fn try_deeplink_login(
    app: &AppHandle,
    acc: &Account,
    cursor_path: Option<&std::path::Path>,
    db_path: &std::path::Path,
) -> Result<(), String> {
    let expected = db::token_hash(&acc.token.access_token);

    // 已经是目标账号的 token：无需投递
    let dbp = db_path.to_path_buf();
    if let Ok(Ok(Some(current))) =
        tokio::task::spawn_blocking(move || db::read_access_token_hash(&dbp)).await
    {
        if current == expected {
            return Ok(());
        }
    }

    emit_switch_progress(app, "deeplink", "正在通知 Cursor 切换账号...", 45, "running");
    let url = deeplink::build_login_url(&acc.token.access_token, &acc.token.refresh_token);
    let path = cursor_path.map(|p| p.to_path_buf());
    tokio::task::spawn_blocking(move || process::open_cursor_url(path.as_deref(), &url))
        .await
        .map_err(|e| format!("Open deeplink task failed: {}", e))??;

    emit_switch_progress(app, "verify", "正在确认 Cursor 已切换...", 60, "running");
    let start = std::time::Instant::now();
    let mut last_err: Option<String> = None;
    while start.elapsed() < std::time::Duration::from_millis(DEEPLINK_VERIFY_TIMEOUT_MS) {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let dbp = db_path.to_path_buf();
        match tokio::task::spawn_blocking(move || db::read_access_token_hash(&dbp)).await {
            Ok(Ok(Some(current))) if current == expected => return Ok(()),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => last_err = Some(e),
            Err(e) => last_err = Some(e.to_string()),
        }
        let elapsed = start.elapsed().as_millis() as u64;
        let p = 60 + (elapsed * 30 / DEEPLINK_VERIFY_TIMEOUT_MS).min(30) as u8;
        emit_switch_progress(app, "verify", "正在确认 Cursor 已切换...", p, "running");
    }

    Err(match last_err {
        Some(e) => format!("{}s 内未确认 Cursor 已切换（{}）", DEEPLINK_VERIFY_TIMEOUT_MS / 1000, e),
        None => format!("{}s 内未确认 Cursor 已切换", DEEPLINK_VERIFY_TIMEOUT_MS / 1000),
    })
}

/// 生成机器码并保存到账号（仅更新账号的机器码信息，不修改当前使用账号）
#[tauri::command]
pub async fn cursor_generate_and_bind_machine_id(
    app: AppHandle,
    account_id: String,
) -> Result<SwitchAccountResponse, String> {
    use crate::cursor::modules::machine::TelemetryIds;

    // 1. 加载账号
    let mut acc = storage::load_account(&app, &account_id).await?;

    // 2. 生成新机器码
    let ids = TelemetryIds::generate();

    // 3. 构造 MachineInfo 并保存到账号
    let machine_info = crate::cursor::models::MachineInfo {
        machine_id: Some(ids.machine_id.clone()),
        mac_machine_id: Some(ids.mac_machine_id.clone()),
        dev_device_id: Some(ids.dev_device_id.clone()),
        sqm_id: Some(ids.sqm_id.clone()),
        storage_service_machine_id: Some(ids.service_machine_id.clone()),
        ..Default::default()
    };
    acc.machine_info = Some(machine_info);
    storage::save_account(&app, &acc).await?;

    Ok(SwitchAccountResponse {
        message: format!("Machine ID generated and bound to account: {}", acc.email),
        warnings: Vec::new(),
    })
}

/// 获取自定义 Cursor 路径
#[tauri::command]
pub async fn cursor_get_custom_path(app: AppHandle) -> Result<Option<String>, String> {
    use crate::core::path_manager::{CURSOR_CONFIG, get_custom_path};
    get_custom_path(&app, &CURSOR_CONFIG)
}

/// 设置自定义 Cursor 路径
#[tauri::command]
pub async fn cursor_set_custom_path(app: AppHandle, path: Option<String>) -> Result<(), String> {
    use crate::core::path_manager::{CURSOR_CONFIG, set_custom_path};
    set_custom_path(&app, &CURSOR_CONFIG, path, |p| {
        process::validate_cursor_path(p)
    })
}

/// 验证 Cursor 路径
#[tauri::command]
pub async fn cursor_validate_path(path: String) -> Result<bool, String> {
    process::validate_cursor_path(&path)
}

/// 获取默认 Cursor 路径
#[tauri::command]
pub async fn cursor_get_default_path() -> Result<String, String> {
    process::get_cursor_executable_path().map(|p| p.to_string_lossy().to_string())
}

/// 打开文件选择对话框选择 Cursor 可执行文件
#[tauri::command]
pub async fn cursor_select_executable_path() -> Result<Option<String>, String> {
    use crate::core::path_manager::{CURSOR_CONFIG, select_executable_path};
    select_executable_path(&CURSOR_CONFIG)
}

/// 验证账号 Token 是否有效
#[tauri::command]
pub async fn cursor_validate_account(app: AppHandle, account_id: String) -> Result<bool, String> {
    let acc = storage::load_account(&app, &account_id).await?;
    let session_token = acc
        .token
        .workos_cursor_session_token
        .as_deref()
        .ok_or_else(|| "No session token available".to_string())?;

    // 尝试获取用户信息来验证 token 是否有效
    match auth::get_user_info(session_token).await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// 批量验证所有账号
// #[tauri::command]
// pub async fn cursor_validate_all_accounts(
//     app: AppHandle,
// ) -> Result<Vec<(String, bool)>, String> {
//     let accounts = storage::list_accounts(&app).await?;
//     let mut results = Vec::new();

//     for acc in accounts {
//         let is_valid = if let Some(session_token) = acc.token.workos_cursor_session_token.as_deref() {

//         } else {

//         };
//         results.push((acc.id, is_valid));
//     }

//     // Ok(results)
// }

/// 获取用量摘要（含 Ultra / Pro+ 的 Grok Bot 周额度）
#[tauri::command]
pub async fn cursor_get_usage_summary(
    session_token: String,
    access_token: Option<String>,
) -> Result<auth::UsageSummary, String> {
    auth::get_usage_summary(&session_token, access_token.as_deref()).await
}

/// 获取账号聚合用量数据
#[tauri::command]
pub async fn cursor_get_aggregated_usage(
    session_token: String,
    start_date: u64,
    end_date: u64,
    team_id: i32,
) -> Result<Option<auth::AggregatedUsageData>, String> {
    auth::get_aggregated_usage_data(&session_token, start_date, end_date, team_id).await
}

/// 获取账号过滤的使用事件
#[tauri::command]
pub async fn cursor_get_filtered_usage_events(
    session_token: String,
    start_date: Option<String>,
    end_date: Option<String>,
    page: Option<i32>,
    page_size: Option<i32>,
    team_id: i32,
) -> Result<Option<auth::FilteredUsageEventsData>, String> {
    auth::get_filtered_usage_events(
        &session_token,
        team_id,
        start_date.as_deref(),
        end_date.as_deref(),
        page,
        page_size,
    )
    .await
}

/// 列出账号的登录设备 / 活跃会话
#[tauri::command]
pub async fn cursor_list_sessions(
    session_token: String,
) -> Result<sessions::CursorSessionList, String> {
    sessions::list_sessions(&session_token).await
}

/// 踢出指定登录设备。`session_type` 传列表里返回的字符串枚举
#[tauri::command]
pub async fn cursor_revoke_session(
    session_token: String,
    session_id: String,
    session_type: Option<String>,
) -> Result<(), String> {
    sessions::revoke_session(&session_token, &session_id, session_type.as_deref()).await
}

/// 从 session token 获取用户信息
/// 前端用于在添加账号前检查邮箱是否存在
#[tauri::command]
pub async fn cursor_get_user_info_from_session(
    session_token: String,
) -> Result<auth::CursorUserInfo, String> {
    auth::get_user_info(&session_token).await
}

/// 刷新已有账号的 token
/// 使用新的 session_token 更新指定账号的认证信息
#[tauri::command]
pub async fn cursor_refresh_account_tokens(
    app: AppHandle,
    account_id: String,
    session_token: String,
) -> Result<Account, String> {
    // 1. 加载现有账号
    let mut account = storage::load_account(&app, &account_id).await?;

    // 2. 获取新的 access token
    let token_response = auth::get_access_token_from_session(&session_token).await?;

    // 3. 解析过期时间
    let expiry_timestamp = parse_jwt_exp(&token_response.access_token)
        .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60);

    let session_expiry_timestamp = parse_session_token_exp(&session_token);

    // 4. 更新 token 数据
    account.token.access_token = token_response.access_token;
    account.token.refresh_token = token_response.refresh_token.unwrap_or_default();
    account.token.expiry_timestamp = expiry_timestamp;
    account.token.workos_cursor_session_token = Some(session_token);
    account.token.session_expiry_timestamp = session_expiry_timestamp;
    // 换了新 session，之前的失效标记不再成立
    account.session_invalid_at = None;
    account.updated_at = chrono::Utc::now().timestamp();

    // 5. 保存更新后的账号
    storage::save_account(&app, &account).await?;

    Ok(account)
}

/// 用新的 access token 覆盖已有账号的凭证（Access Token 添加方式的「覆盖」分支）。
///
/// 与 `cursor_add_account_with_access_token` 走同一套校验：先拿 Stripe 订阅确认 token 可用，
/// 再写回 access/refresh/expiry 与套餐。原有的 session token 与用量摘要保持不变——
/// 它们属于同一个邮箱，换 access token 不影响其有效性。
#[tauri::command]
pub async fn cursor_refresh_account_access_token(
    app: AppHandle,
    account_id: String,
    access_token: String,
) -> Result<Account, String> {
    let access_token = access_token.trim().to_string();
    if access_token.is_empty() {
        return Err("Access token is required".to_string());
    }

    let mut account = storage::load_account(&app, &account_id).await?;

    let profile = auth::get_stripe_profile(&access_token).await?;
    let membership = profile.plan_type();

    account.token.expiry_timestamp =
        parse_jwt_exp(&access_token).unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60);
    account.token.access_token = access_token.clone();
    account.token.refresh_token = access_token;
    if membership.is_some() {
        account.membership_type = membership;
    }
    account.updated_at = chrono::Utc::now().timestamp();

    storage::save_account(&app, &account).await?;
    Ok(account)
}

/// 使用 session token 添加账号（自动获取 accessToken）
///
/// 一站式添加账号：传入 session cookie，自动完成：
/// 1. 获取订阅信息
/// 2. PKCE 流程获取 accessToken
/// 3. 保存账号
///
/// 注意：邮箱重复检查由前端负责
#[tauri::command]
pub async fn cursor_add_account_with_session(
    app: AppHandle,
    session_token: String,
) -> Result<Account, String> {
    // 1. 获取用户信息（使用 session_token + Cookie 认证）
    let user_info = auth::get_user_info(&session_token).await?;

    // 2. 获取 accessToken
    let token_response = auth::get_access_token_from_session(&session_token).await?;

    // 5. 解析 JWT 中的过期时间
    let expiry_timestamp = parse_jwt_exp(&token_response.access_token)
        .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60); // 默认 60 天

    // 3. 解析 session token 的过期时间
    let session_expiry_timestamp = parse_session_token_exp(&session_token);

    // 4. 创建 Token 数据
    let token = TokenData::new(
        token_response.access_token,
        token_response.refresh_token.unwrap_or_default(),
        expiry_timestamp,
        Some(user_info.id.clone()),
        Some(session_token),
        session_expiry_timestamp,
    );

    // 5. 创建账号
    let account_id = uuid::Uuid::new_v4().to_string();
    let mut account = Account::new(account_id, user_info.email, token);
    account.name = user_info.name;

    // 6. 保存账号
    storage::save_account(&app, &account).await?;

    Ok(account)
}

/// 使用 access token 添加账号（邮箱由前端传入，订阅类型通过 full_stripe_profile 获取）
#[tauri::command]
pub async fn cursor_add_account_with_access_token(
    app: AppHandle,
    email: String,
    access_token: String,
) -> Result<Account, String> {
    let email = email.trim().to_string();
    let access_token = access_token.trim().to_string();

    if email.is_empty() {
        return Err("Email is required".to_string());
    }

    // 1. 调用 full_stripe_profile 获取订阅信息
    let profile = auth::get_stripe_profile(&access_token).await?;

    // 与 usage-summary 同口径（pro_student 不会被 individualMembershipType=pro 盖掉）
    let membership = profile.plan_type();

    // 2. 解析 JWT 过期时间
    let expiry_timestamp =
        parse_jwt_exp(&access_token).unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60);

    // 3. 创建 Token（refresh_token = access_token，无 session）
    let token = TokenData::new(
        access_token.clone(),
        access_token,
        expiry_timestamp,
        None,
        None,
        None,
    );

    // 4. 创建账号并设置 membership_type
    let account_id = uuid::Uuid::new_v4().to_string();
    let mut account = Account::new(account_id, email, token);
    account.membership_type = membership;

    // 5. 保存账号
    storage::save_account(&app, &account).await?;

    Ok(account)
}

/// 更新账号信息
#[tauri::command]
pub async fn cursor_update_account(app: AppHandle, account: Account) -> Result<(), String> {
    storage::save_account(&app, &account).await
}

// ==================== 导入功能相关 ====================

/// 导入账号请求中的 auth_info 结构
#[derive(Debug, Clone, Deserialize)]
pub struct ImportAuthInfo {
    #[serde(rename = "WorkosCursorSessionToken")]
    pub workos_cursor_session_token: Option<String>,
    #[serde(rename = "cursorAuth/accessToken")]
    pub access_token: Option<String>,
    #[serde(rename = "cursorAuth/refreshToken")]
    pub refresh_token: Option<String>,
}

/// 导入账号请求中的单个账号数据
#[derive(Debug, Clone, Deserialize)]
pub struct ImportAccountData {
    pub email: String,
    pub auth_info: Option<ImportAuthInfo>,
    pub machine_info: Option<MachineInfo>,
}

/// 导入结果
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub success: bool,
    pub email: String,
    pub error: Option<String>,
    pub account: Option<Account>,
}

/// 批量导入结果
#[derive(Debug, Clone, Serialize)]
pub struct BatchImportResult {
    pub total: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub results: Vec<ImportResult>,
}

/// 批量导入账号
/// 从 JSON 数据中导入账号，优先使用直接提供的 accessToken/refreshToken
/// 如果没有则尝试使用 WorkosCursorSessionToken 获取
#[tauri::command]
pub async fn cursor_import_accounts(
    app: AppHandle,
    accounts_data: Vec<ImportAccountData>,
) -> Result<BatchImportResult, String> {
    let total = accounts_data.len();
    let mut results = Vec::new();
    let mut success_count = 0;
    let mut failed_count = 0;

    // 获取现有账号列表用于检查重复
    let existing_accounts = storage::list_accounts(&app).await?;
    let existing_emails: std::collections::HashSet<String> = existing_accounts
        .iter()
        .map(|a| a.email.trim().to_lowercase())
        .collect();

    for data in accounts_data {
        let email = data.email.clone();
        let email_lower = email.trim().to_lowercase();

        // 检查邮箱是否已存在
        if existing_emails.contains(&email_lower) {
            results.push(ImportResult {
                success: false,
                email: email.clone(),
                error: Some(format!("Account with email '{}' already exists", email)),
                account: None,
            });
            failed_count += 1;
            continue;
        }

        // 获取 auth_info
        let auth_info = match &data.auth_info {
            Some(info) => info,
            None => {
                results.push(ImportResult {
                    success: false,
                    email: email.clone(),
                    error: Some("Missing auth_info".to_string()),
                    account: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // 检查是否有直接的 accessToken 和 refreshToken
        let has_direct_tokens = auth_info
            .access_token
            .as_ref()
            .map_or(false, |t| !t.is_empty())
            && auth_info
                .refresh_token
                .as_ref()
                .map_or(false, |t| !t.is_empty());

        let import_result = if has_direct_tokens {
            // 直接使用提供的 accessToken 和 refreshToken（Session 可选；空字符串视为无）
            let session_opt = auth_info
                .workos_cursor_session_token
                .clone()
                .filter(|s| !s.trim().is_empty());
            import_account_with_tokens(
                &app,
                &email,
                auth_info.access_token.as_ref().unwrap(),
                auth_info.refresh_token.as_ref().unwrap(),
                session_opt,
                data.machine_info.clone(),
            )
            .await
        } else if let Some(ref session_token) = auth_info.workos_cursor_session_token {
            if !session_token.is_empty() {
                // 使用 session token 获取 access token
                import_account_with_session_token(
                    &app,
                    &email,
                    session_token,
                    data.machine_info.clone(),
                )
                .await
            } else {
                Err(
                    "Missing valid auth credentials (accessToken or WorkosCursorSessionToken)"
                        .to_string(),
                )
            }
        } else {
            Err(
                "Missing valid auth credentials (accessToken or WorkosCursorSessionToken)"
                    .to_string(),
            )
        };

        match import_result {
            Ok(account) => {
                results.push(ImportResult {
                    success: true,
                    email: email.clone(),
                    error: None,
                    account: Some(account),
                });
                success_count += 1;
            }
            Err(e) => {
                results.push(ImportResult {
                    success: false,
                    email: email.clone(),
                    error: Some(e),
                    account: None,
                });
                failed_count += 1;
            }
        }
    }

    Ok(BatchImportResult {
        total,
        success_count,
        failed_count,
        results,
    })
}

/// 使用直接提供的 accessToken 和 refreshToken 导入账号
async fn import_account_with_tokens(
    app: &AppHandle,
    email: &str,
    access_token: &str,
    refresh_token: &str,
    session_token: Option<String>,
    machine_info: Option<MachineInfo>,
) -> Result<Account, String> {
    // 1. 解析 JWT 中的过期时间
    let expiry_timestamp =
        parse_jwt_exp(access_token).unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60); // 默认 60 天

    // 2. 解析 session token 的过期时间（如果有的话）
    let session_expiry_timestamp = session_token
        .as_ref()
        .and_then(|t| parse_session_token_exp(t));

    // 3. 尝试从 session token 获取用户信息（用于获取 user_id）
    let user_id = if let Some(ref token) = session_token {
        match auth::get_user_info(token).await {
            Ok(user_info) => Some(user_info.id),
            Err(_) => None,
        }
    } else {
        None
    };

    let no_session = session_token
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true);

    // 4. 创建 Token 数据
    let token = TokenData::new(
        access_token.to_string(),
        refresh_token.to_string(),
        expiry_timestamp,
        user_id,
        session_token,
        session_expiry_timestamp,
    );

    // 5. 创建账号（带机器码信息）
    let account_id = uuid::Uuid::new_v4().to_string();
    let mut account =
        Account::new_with_machine_info(account_id, email.to_string(), token, machine_info);

    // 无 Workos Session 时拉取 Stripe 订阅（与 accessToken 添加路径一致）
    if no_session {
        if let Ok(profile) = auth::get_stripe_profile(access_token).await {
            account.membership_type = profile.plan_type();
        }
    }

    // 6. 保存账号
    storage::save_account(app, &account).await?;

    Ok(account)
}

/// 使用 session token 获取 access token 并导入账号
async fn import_account_with_session_token(
    app: &AppHandle,
    email: &str,
    session_token: &str,
    machine_info: Option<MachineInfo>,
) -> Result<Account, String> {
    // 1. 获取 accessToken
    let token_response = auth::get_access_token_from_session(session_token).await?;

    // 2. 解析 JWT 中的过期时间
    let expiry_timestamp = parse_jwt_exp(&token_response.access_token)
        .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400 * 60); // 默认 60 天

    // 3. 解析 session token 的过期时间
    let session_expiry_timestamp = parse_session_token_exp(session_token);

    // 4. 尝试获取用户信息（用于获取 user_id）
    let user_id = match auth::get_user_info(session_token).await {
        Ok(user_info) => Some(user_info.id),
        Err(_) => None,
    };

    // 5. 创建 Token 数据
    let token = TokenData::new(
        token_response.access_token,
        token_response.refresh_token.unwrap_or_default(),
        expiry_timestamp,
        user_id,
        Some(session_token.to_string()),
        session_expiry_timestamp,
    );

    // 6. 创建账号（带机器码信息）
    let account_id = uuid::Uuid::new_v4().to_string();
    let account =
        Account::new_with_machine_info(account_id, email.to_string(), token, machine_info);

    // 7. 保存账号
    storage::save_account(app, &account).await?;

    Ok(account)
}
/// 检查 Cursor 自动更新是否已禁用
#[tauri::command]
pub async fn cursor_check_auto_update_disabled(app: AppHandle) -> Result<bool, String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};
    let custom_path = read_custom_path_from_config(&app, &CURSOR_CONFIG);
    machine::check_auto_update_disabled(custom_path.as_deref())
}

/// 禁用 Cursor 自动更新
#[tauri::command]
pub async fn cursor_disable_auto_update(app: AppHandle) -> Result<(), String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};
    let custom_path = read_custom_path_from_config(&app, &CURSOR_CONFIG);
    machine::disable_auto_update(custom_path.as_deref())
}

/// 启用 Cursor 自动更新（从备份恢复）
#[tauri::command]
pub async fn cursor_enable_auto_update(app: AppHandle) -> Result<(), String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};
    let custom_path = read_custom_path_from_config(&app, &CURSOR_CONFIG);
    machine::enable_auto_update(custom_path.as_deref())
}

/// 检查 main.js 文件是否有写入权限（macOS App Management 权限）
#[tauri::command]
pub async fn cursor_check_main_js_permission(app: AppHandle) -> Result<bool, String> {
    use crate::core::path_manager::{CURSOR_CONFIG, read_custom_path_from_config};
    let custom_path = read_custom_path_from_config(&app, &CURSOR_CONFIG);
    machine::check_main_js_writable(custom_path.as_deref())
}

/// 导出账号数据（单个或批量）
/// 返回 JSON 字符串，格式与导入格式兼容
#[tauri::command]
pub async fn cursor_export_accounts(
    app: AppHandle,
    account_ids: Option<Vec<String>>,
) -> Result<String, String> {
    use crate::cursor::models::ExportAccountData;
    use serde_json::to_string_pretty;

    // 如果没有指定账号 ID，导出所有账号
    let accounts = if let Some(ids) = account_ids {
        // 导出指定账号
        let mut result = Vec::new();
        for id in ids {
            match storage::load_account(&app, &id).await {
                Ok(acc) => result.push(acc),
                Err(_) => continue, // 跳过加载失败的账号
            }
        }
        result
    } else {
        // 导出所有账号
        storage::list_accounts(&app).await?
    };

    // 转换为导出格式
    let export_data: Vec<ExportAccountData> = accounts
        .into_iter()
        .map(|acc| ExportAccountData::from_account(&acc))
        .collect();

    to_string_pretty(&export_data).map_err(|e| format!("Failed to serialize accounts: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(plan: Option<&str>, access: &str) -> Account {
        let token = TokenData::new(access.into(), access.into(), 100, None, None, None);
        let mut acc = Account::new("id-1".into(), "a@b.c".into(), token);
        acc.membership_type = plan.map(str::to_string);
        acc
    }

    #[test]
    fn switch_token_refresh_keeps_stored_plan() {
        // 存储里是刷新配额得到的 pro_student；切号流程手上的副本即便是 pro，也不能写回
        let mut latest = account(Some("pro_student"), "old");
        let mut refreshed = account(Some("pro"), "new");
        refreshed.session_invalid_at = None;
        refreshed.updated_at = 42;
        latest.session_invalid_at = Some(1);

        merge_refreshed_token(&mut latest, &refreshed);

        assert_eq!(latest.membership_type.as_deref(), Some("pro_student"));
        assert_eq!(latest.token.access_token, "new");
        assert_eq!(latest.session_invalid_at, None);
        assert_eq!(latest.updated_at, 42);
    }
}
