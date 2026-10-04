//! Cursor state.vscdb 数据库操作模块

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 获取 Cursor 数据库路径（跨平台）
pub fn get_db_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or("Cannot get home directory")?;
        Ok(home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"))
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| "Cannot get APPDATA environment variable".to_string())?;
        Ok(PathBuf::from(appdata).join("Cursor\\User\\globalStorage\\state.vscdb"))
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or("Cannot get home directory")?;
        Ok(home.join(".config/Cursor/User/globalStorage/state.vscdb"))
    }
}

/// 获取 Cursor storage.json 路径
pub fn get_storage_json_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or("Cannot get home directory")?;
        Ok(home.join("Library/Application Support/Cursor/User/globalStorage/storage.json"))
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| "Cannot get APPDATA environment variable".to_string())?;
        Ok(PathBuf::from(appdata).join("Cursor\\User\\globalStorage\\storage.json"))
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or("Cannot get home directory")?;
        Ok(home.join(".config/Cursor/User/globalStorage/storage.json"))
    }
}

/// 检查数据库是否存在
pub fn check_db_exists() -> bool {
    match get_db_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    }
}

/// 等待 Cursor 写锁的最长时间。Cursor 刚退出时 WAL 可能还在落盘，直接写会撞 SQLITE_BUSY
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// 切换账号时需要清掉的「账号相关」键。
///
/// 只清这些，而不是 `cursorAuth/%` 全删：同一前缀下还有用户自己填的模型 Key
/// （openAIKey / claudeKey / googleKey / azureApiKey / bedrock*）和 onboardingDate 等，
/// 换账号不应该把它们一起抹掉。
pub const ACCOUNT_SCOPED_AUTH_KEYS: &[&str] = &[
    "cursorAuth/accessToken",
    "cursorAuth/refreshToken",
    "cursorAuth/cachedEmail",
    "cursorAuth/cachedSignUpType",
    "cursorAuth/stripeMembershipType",
    "cursorAuth/stripeSubscriptionStatus",
    "cursorAuth/stripeMembershipAuthId",
    "cursorAuth/stripeCustomerId",
    "cursorAuth/cachedTeam",
    "cursorAuth/cachedScopedProfile",
    "cursorAuth/teamId",
];

/// 以读写方式打开 state.vscdb（不存在时报错，避免 rusqlite 悄悄建一个空库）
fn open_rw(db_path: &Path) -> Result<Connection, String> {
    if !db_path.exists() {
        return Err(format!(
            "Cursor database not found: {} (please start Cursor once first)",
            db_path.display()
        ));
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open database: {}", e))?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| format!("Failed to set busy timeout: {}", e))?;
    Ok(conn)
}

/// 以只读方式打开 state.vscdb（Cursor 运行中也能读，WAL 模式下不阻塞对方）
fn open_ro(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open database read-only: {}", e))?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|e| format!("Failed to set busy timeout: {}", e))?;
    Ok(conn)
}

/// 把 WAL 合并回主库并截断（失败不影响已提交的数据，只是 WAL 文件留着）
fn checkpoint_wal(conn: &Connection) {
    if let Err(e) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
        eprintln!("wal_checkpoint failed (ignored): {}", e);
    }
}

/// ItemTable.value 可能是 TEXT 也可能是 BLOB，统一转成字符串
fn value_to_string(value: rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(s) => Some(s),
        rusqlite::types::Value::Blob(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        _ => None,
    }
}

/// token 的 sha256（十六进制）。只用于比对，永远不要打印 token 本身
pub fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn write_item(tx: &rusqlite::Transaction<'_>, key: &str, value: &str) -> Result<(), String> {
    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        [key, value],
    )
    .map_err(|e| format!("Failed to write key {}: {}", key, e))?;
    Ok(())
}

/// 写入 Cursor 认证状态到数据库（调用方需保证 Cursor 已退出）
/// Cursor 使用 cursorAuth/accessToken 和 cursorAuth/refreshToken 存储认证信息
pub fn write_cursor_auth_state(
    db_path: &PathBuf,
    access_token: &str,
    refresh_token: &str,
    email: &str,
) -> Result<(), String> {
    println!("Writing auth state to database: {}", db_path.display());

    let mut conn = open_rw(db_path)?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to start transaction: {}", e))?;

    // 只清理账号相关的认证状态，保留用户自己的模型 Key 等
    for key in ACCOUNT_SCOPED_AUTH_KEYS {
        tx.execute("DELETE FROM ItemTable WHERE key = ?", [key])
            .map_err(|e| format!("Failed to clear {}: {}", key, e))?;
    }
    println!("Cleared old account-scoped cursorAuth entries");

    // 写入新的认证信息
    // Cursor 存储格式: cursorAuth/accessToken, cursorAuth/refreshToken
    write_item(&tx, "cursorAuth/accessToken", access_token)?;
    write_item(&tx, "cursorAuth/refreshToken", refresh_token)?;
    println!("Written access_token and refresh_token");

    // 写入邮箱缓存
    write_item(&tx, "cursorAuth/cachedEmail", email)?;
    println!("Written cached email: {}", email);

    tx.commit()
        .map_err(|e| format!("Failed to commit auth updates: {}", e))?;
    checkpoint_wal(&conn);
    println!("Auth state written successfully for: {}", email);
    Ok(())
}

/// 只更新邮箱缓存（无感切换后使用，Cursor 运行中也可写，失败由调用方当作警告）
pub fn write_cached_email(db_path: &Path, email: &str) -> Result<(), String> {
    let conn = open_rw(db_path)?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('cursorAuth/cachedEmail', ?)",
        [email],
    )
    .map_err(|e| format!("Failed to write cached email: {}", e))?;
    Ok(())
}

/// 读取 Cursor 当前 accessToken 的 sha256（只读，不返回 token 本身）
pub fn read_access_token_hash(db_path: &Path) -> Result<Option<String>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open_ro(db_path)?;
    let value = conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
        [],
        |row| row.get::<_, rusqlite::types::Value>(0),
    );
    match value {
        Ok(v) => Ok(value_to_string(v)
            .filter(|s| !s.is_empty())
            .map(|s| token_hash(&s))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to read access token: {}", e)),
    }
}

/// 导出当前 cursorAuth/* 到 JSON 备份文件（替代整库复制成 state.vscdb.backup）。
///
/// `state.vscdb.backup` 是 Cursor 自己的损坏恢复文件，库大时还有好几 GB；
/// 这里只备份认证相关的几行，写到本工具的数据目录，保留最近 `keep` 份。
pub fn export_auth_backup(db_path: &Path, backup_dir: &Path, keep: usize) -> Result<PathBuf, String> {
    if !db_path.exists() {
        return Err(format!("Cursor database not found: {}", db_path.display()));
    }
    let conn = open_ro(db_path)?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM ItemTable WHERE key LIKE 'cursorAuth/%' ORDER BY key")
        .map_err(|e| format!("Failed to query cursorAuth entries: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, rusqlite::types::Value>(1)?))
        })
        .map_err(|e| format!("Failed to query cursorAuth entries: {}", e))?;

    let mut keys = serde_json::Map::new();
    for row in rows {
        let (key, value) = row.map_err(|e| format!("Failed to read row: {}", e))?;
        if let Some(v) = value_to_string(value) {
            keys.insert(key, serde_json::Value::String(v));
        }
    }

    std::fs::create_dir_all(backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;
    let now = chrono::Local::now();
    let file = backup_dir.join(format!("cursorAuth-{}.json", now.format("%Y%m%d-%H%M%S-%3f")));
    let payload = serde_json::json!({
        "exported_at": now.to_rfc3339(),
        "source": db_path.display().to_string(),
        "keys": keys,
    });
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize backup: {}", e))?;
    std::fs::write(&file, json).map_err(|e| format!("Failed to write backup: {}", e))?;

    // 只保留最近 keep 份
    if let Ok(entries) = std::fs::read_dir(backup_dir) {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("cursorAuth-") && n.ends_with(".json"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        if files.len() > keep {
            for old in &files[..files.len() - keep] {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    Ok(file)
}

/// 等待 state.vscdb 的写锁被释放（Cursor 进程退出后 WAL 可能还被占用一小会）。
/// 能拿到 IMMEDIATE 写锁即视为已释放，并顺手做一次 WAL checkpoint。
pub fn wait_for_db_release(db_path: &Path, timeout: Duration) -> Result<(), String> {
    if !db_path.exists() {
        return Ok(());
    }
    let start = Instant::now();
    loop {
        let attempt = (|| -> Result<(), String> {
            let conn = Connection::open_with_flags(
                db_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| e.to_string())?;
            conn.busy_timeout(Duration::from_millis(300))
                .map_err(|e| e.to_string())?;
            conn.execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
                .map_err(|e| e.to_string())?;
            checkpoint_wal(&conn);
            Ok(())
        })();
        let last_err = match attempt {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        if start.elapsed() >= timeout {
            return Err(format!(
                "Cursor database is still locked after {}s: {}",
                timeout.as_secs(),
                last_err
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// 重置 state.vscdb 中的机器标识符
/// - storage.serviceMachineId: 如果提供了 service_machine_id 则使用，否则生成新的 UUID
/// - workbench.experiments.statsigBootstrap.customIDs.stableID: 使用 machine_id (64字符十六进制)
pub fn reset_machine_ids_in_db(
    machine_id: &str,
    service_machine_id: Option<&str>,
) -> Result<(), String> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        // 数据库不存在，静默返回（Cursor 可能未运行过）
        return Ok(());
    }

    println!("Resetting machine IDs in database: {}", db_path.display());

    let mut conn = open_rw(&db_path)?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to start transaction: {}", e))?;

    // 1. 重置 storage.serviceMachineId (使用提供的值或生成新 UUID)
    let new_service_id = if let Some(service_id) = service_machine_id {
        service_id.to_string()
    } else {
        use uuid::Uuid;
        Uuid::new_v4().to_string()
    };
    write_item(&tx, "storage.serviceMachineId", &new_service_id)?;
    println!("Written storage.serviceMachineId: {}", new_service_id);

    // 2. 重置 statsigBootstrap.customIDs.stableID (使用与 machineId 相同的值)
    // 先读取现有的 statsigBootstrap 数据
    match tx.query_row(
        "SELECT value FROM ItemTable WHERE key = 'workbench.experiments.statsigBootstrap'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(existing_value) => {
            // 解析并更新 JSON
            if let Ok(mut statsig_data) = serde_json::from_str::<serde_json::Value>(&existing_value)
            {
                if let Some(user_obj) = statsig_data.get_mut("user") {
                    if let Some(custom_ids) = user_obj.get_mut("customIDs") {
                        if let Some(id_map) = custom_ids.as_object_mut() {
                            id_map.insert(
                                "stableID".to_string(),
                                serde_json::Value::String(machine_id.to_string()),
                            );
                        }
                    } else if let Some(user_obj) = statsig_data["user"].as_object_mut() {
                        // customIDs 不存在，创建它
                        user_obj.insert(
                            "customIDs".to_string(),
                            serde_json::json!({
                                "stableID": machine_id
                            }),
                        );
                    }
                }
                // 写回更新后的 JSON
                let updated_json = serde_json::to_string(&statsig_data)
                    .map_err(|e| format!("Failed to serialize statsigBootstrap: {}", e))?;
                tx.execute(
                    "UPDATE ItemTable SET value = ? WHERE key = 'workbench.experiments.statsigBootstrap'",
                    [&updated_json],
                ).map_err(|e| format!("Failed to update statsigBootstrap: {}", e))?;
                println!(
                    "Written statsigBootstrap.customIDs.stableID: {}",
                    machine_id
                );
            }
        }
        Err(_) => {
            // statsigBootstrap 不存在，创建新的
            let new_statsig_data = serde_json::json!({
                "user": {
                    "customIDs": {
                        "stableID": machine_id
                    }
                }
            });
            let new_statsig_json = serde_json::to_string(&new_statsig_data)
                .map_err(|e| format!("Failed to serialize statsigBootstrap: {}", e))?;
            tx.execute(
                "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('workbench.experiments.statsigBootstrap', ?)",
                [&new_statsig_json],
            ).map_err(|e| format!("Failed to insert statsigBootstrap: {}", e))?;
            println!(
                "Created new statsigBootstrap.customIDs.stableID: {}",
                machine_id
            );
        }
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit machine ID updates: {}", e))?;
    checkpoint_wal(&conn);
    println!("Machine IDs in database reset successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atm-cursor-db-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.vscdb");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
        )
        .unwrap();
        path
    }

    fn get(path: &Path, key: &str) -> Option<String> {
        let conn = Connection::open(path).unwrap();
        conn.query_row("SELECT value FROM ItemTable WHERE key = ?", [key], |r| {
            r.get::<_, rusqlite::types::Value>(0)
        })
        .ok()
        .and_then(value_to_string)
    }

    #[test]
    fn auth_write_keeps_user_model_keys() {
        let path = temp_db("write");
        {
            let conn = Connection::open(&path).unwrap();
            for (k, v) in [
                ("cursorAuth/accessToken", "old-at"),
                ("cursorAuth/refreshToken", "old-rt"),
                ("cursorAuth/stripeMembershipType", "pro"),
                ("cursorAuth/teamId", "42"),
                ("cursorAuth/openAIKey", "sk-user"),
                ("cursorAuth/onboardingDate", "2025-01-01"),
            ] {
                conn.execute("INSERT INTO ItemTable VALUES (?, ?)", [k, v]).unwrap();
            }
        }

        write_cursor_auth_state(&path, "new-at", "new-rt", "a@b.c").unwrap();

        assert_eq!(get(&path, "cursorAuth/accessToken").as_deref(), Some("new-at"));
        assert_eq!(get(&path, "cursorAuth/refreshToken").as_deref(), Some("new-rt"));
        assert_eq!(get(&path, "cursorAuth/cachedEmail").as_deref(), Some("a@b.c"));
        assert_eq!(get(&path, "cursorAuth/stripeMembershipType"), None);
        assert_eq!(get(&path, "cursorAuth/teamId"), None);
        assert_eq!(get(&path, "cursorAuth/openAIKey").as_deref(), Some("sk-user"));
        assert_eq!(get(&path, "cursorAuth/onboardingDate").as_deref(), Some("2025-01-01"));

        assert_eq!(
            read_access_token_hash(&path).unwrap(),
            Some(token_hash("new-at"))
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_db_is_an_error_not_a_new_file() {
        let path = std::env::temp_dir()
            .join(format!("atm-cursor-db-missing-{}", std::process::id()))
            .join("state.vscdb");
        assert!(write_cursor_auth_state(&path, "a", "b", "c").is_err());
        assert!(!path.exists());
        assert_eq!(read_access_token_hash(&path).unwrap(), None);
    }

    #[test]
    fn backup_exports_only_cursor_auth_and_rotates() {
        let path = temp_db("backup");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute("INSERT INTO ItemTable VALUES ('cursorAuth/cachedEmail', 'x@y.z')", [])
                .unwrap();
            conn.execute("INSERT INTO ItemTable VALUES ('other/key', 'v')", []).unwrap();
        }
        let dir = path.parent().unwrap().join("backups");
        for _ in 0..4 {
            export_auth_backup(&path, &dir, 2).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(files.len(), 2);
        let content = std::fs::read_to_string(files[0].path()).unwrap();
        assert!(content.contains("cursorAuth/cachedEmail"));
        assert!(!content.contains("other/key"));

        wait_for_db_release(&path, Duration::from_secs(1)).unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
