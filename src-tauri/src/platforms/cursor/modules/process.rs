//! Cursor 进程管理模块

use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[cfg(target_os = "macos")]
fn is_helper_process(name: &str, args: &str) -> bool {
    let name = name.to_lowercase();
    let args = args.to_lowercase();
    args.contains("--type=")
        || name.contains("helper")
        || name.contains("plugin")
        || name.contains("renderer")
        || name.contains("gpu")
        || name.contains("crashpad")
        || name.contains("utility")
        || name.contains("audio")
        || name.contains("sandbox")
        || name.contains("language_server")
}

fn is_cursor_process(process: &sysinfo::Process) -> bool {
    let name = process.name().to_string_lossy().to_lowercase();

    #[cfg(target_os = "windows")]
    {
        if name == "cursor.exe" {
            return true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if name == "cursor" || name.starts_with("cursor helper") {
            return true;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if name == "cursor" {
            return true;
        }
    }

    let exe_path = process
        .exe()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_lowercase();

    #[cfg(target_os = "macos")]
    {
        return exe_path.contains("/cursor.app/");
    }

    #[cfg(target_os = "windows")]
    {
        return exe_path.ends_with("cursor.exe");
    }

    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&exe_path)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|f| f.eq_ignore_ascii_case("cursor"))
            .unwrap_or(false)
    }
}

fn refresh_processes_all(sys: &mut System) {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());
}

fn any_cursor_process(sys: &System) -> bool {
    sys.processes().values().any(is_cursor_process)
}

/// 刷新进程表后判断 Cursor 是否仍在运行（复用 `System`，避免反复 `System::new()`）
fn cursor_still_running(sys: &mut System) -> bool {
    refresh_processes_all(sys);
    any_cursor_process(sys)
}

fn collect_cursor_pids(sys: &System) -> Vec<u32> {
    let mut pids = Vec::new();
    for (pid, process) in sys.processes() {
        if is_cursor_process(process) {
            pids.push(pid.as_u32());
        }
    }
    pids
}

fn get_cursor_pids_with_sys(sys: &mut System) -> Vec<u32> {
    refresh_processes_all(sys);
    collect_cursor_pids(sys)
}

/// 检查 Cursor 是否正在运行
/// 包括主进程和所有 helper 进程，只要有任何一个在运行就返回 true
pub fn is_cursor_running() -> bool {
    let mut sys = System::new();
    cursor_still_running(&mut sys)
}

/// 进程关闭结果
pub struct CloseResult {
    pub success: bool,
    pub warning: Option<String>,
}

/// 温和关闭 Cursor（带验证）
pub fn close_cursor(timeout_secs: u64) -> Result<(), String> {
    let result = close_cursor_with_result(timeout_secs);
    if !result.success {
        if let Some(warning) = result.warning {
            return Err(warning);
        }
    }
    Ok(())
}

/// 温和关闭 Cursor（返回详细结果）
pub fn close_cursor_with_result(timeout_secs: u64) -> CloseResult {
    #[cfg(target_os = "macos")]
    {
        let mut sys = System::new();
        let pids = get_cursor_pids_with_sys(&mut sys);
        if pids.is_empty() {
            return CloseResult {
                success: true,
                warning: None,
            };
        }

        // macOS: 尝试优雅退出
        let _ = Command::new("osascript")
            .args(["-e", "tell application \"Cursor\" to quit"])
            .output();

        // 等待优雅退出，使用 30% 的 timeout_secs，最多 5 秒
        let quit_wait = std::cmp::min(timeout_secs * 3 / 10, 5);
        let start_quit = std::time::Instant::now();
        while start_quit.elapsed() < std::time::Duration::from_secs(quit_wait) {
            if !cursor_still_running(&mut sys) {
                return CloseResult {
                    success: true,
                    warning: None,
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // 如果还在运行，发送 SIGTERM（复用同一 System，用刷新后的 PID）
        refresh_processes_all(&mut sys);
        let current_pids = collect_cursor_pids(&sys);

        let mut main_pid: Option<u32> = None;
        for pid_u32 in &current_pids {
            let pid = sysinfo::Pid::from_u32(*pid_u32);
            if let Some(process) = sys.process(pid) {
                let name = process.name().to_string_lossy();
                let args = process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<String>>()
                    .join(" ");

                if !is_helper_process(&name, &args) {
                    main_pid = Some(*pid_u32);
                    break;
                }
            }
        }

        if let Some(pid) = main_pid {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .output();
        } else {
            for pid in &current_pids {
                let _ = Command::new("kill")
                    .args(["-15", &pid.to_string()])
                    .output();
            }
        }

        // 等待 SIGTERM 生效，使用 40% 的 timeout_secs，最多 5 秒
        let sigterm_wait = std::cmp::min((timeout_secs * 4) / 10, 5);
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(sigterm_wait) {
            if !cursor_still_running(&mut sys) {
                return CloseResult {
                    success: true,
                    warning: None,
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // 强制杀死 - 多次尝试确保进程被彻底杀死
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            refresh_processes_all(&mut sys);
            if !any_cursor_process(&sys) {
                break;
            }
            let remaining = collect_cursor_pids(&sys);
            for pid in remaining {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }

            attempts += 1;
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // 最终验证
        if cursor_still_running(&mut sys) {
            return CloseResult {
                success: false,
                warning: Some("Cursor process still running after forced kill".to_string()),
            };
        }

        CloseResult {
            success: true,
            warning: None,
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut sys = System::new();
        let pids = get_cursor_pids_with_sys(&mut sys);
        if pids.is_empty() {
            return CloseResult {
                success: true,
                warning: None,
            };
        }

        // Windows: 使用 taskkill 优雅关闭（带子进程树）
        let _ = Command::new("taskkill")
            .args(["/IM", "Cursor.exe", "/T"])
            .output();

        // 等待优雅退出，使用 70% 的 timeout_secs，最多 12 秒。
        // Cursor 退出时要把 state.vscdb 刷盘并写 state.vscdb.backup，库大时 3 秒根本不够，
        // 过早 /F 强杀容易留下半截 WAL，下次启动被判损坏后回滚到旧备份（表现为切号失效）
        let graceful_wait = std::cmp::min((timeout_secs * 7) / 10, 12);
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(graceful_wait) {
            if !cursor_still_running(&mut sys) {
                return CloseResult {
                    success: true,
                    warning: None,
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        // 强制关闭 - 多次尝试
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            refresh_processes_all(&mut sys);
            if !any_cursor_process(&sys) {
                break;
            }
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/IM", "Cursor.exe"])
                .output();

            attempts += 1;
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // 最终验证
        if cursor_still_running(&mut sys) {
            return CloseResult {
                success: false,
                warning: Some("Cursor process still running after forced kill".to_string()),
            };
        }

        CloseResult {
            success: true,
            warning: None,
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut sys = System::new();
        let pids = get_cursor_pids_with_sys(&mut sys);
        if pids.is_empty() {
            return CloseResult {
                success: true,
                warning: None,
            };
        }

        // 第一阶段: 发送 SIGTERM (优雅退出)
        for pid in &pids {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .output();
        }

        // 等待 SIGTERM 生效，使用 70% 的 timeout_secs，最多 5 秒
        let graceful_timeout = std::cmp::min((timeout_secs * 7) / 10, 5);
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(graceful_timeout) {
            if !cursor_still_running(&mut sys) {
                return CloseResult {
                    success: true,
                    warning: None,
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // 第二阶段: 强制杀死 (SIGKILL) - 多次尝试确保彻底杀死
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            refresh_processes_all(&mut sys);
            if !any_cursor_process(&sys) {
                break;
            }
            let remaining = collect_cursor_pids(&sys);
            for pid in remaining {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }

            attempts += 1;
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // 最终验证
        if cursor_still_running(&mut sys) {
            return CloseResult {
                success: false,
                warning: Some("Cursor process still running after forced kill".to_string()),
            };
        }

        CloseResult {
            success: true,
            warning: None,
        }
    }
}

/// 杀死所有 Cursor 进程 (SIGKILL)
pub fn kill_cursor_processes() -> Result<(), String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());

    let mut killed_count = 0;

    for (pid, process) in sys.processes() {
        if is_cursor_process(process) {
            if process.kill() {
                killed_count += 1;
                println!(
                    "Killed Cursor process: {} (PID: {})",
                    process.name().to_string_lossy(),
                    pid
                );
            }
        }
    }

    if killed_count > 0 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        Ok(())
    } else {
        Err("No Cursor processes found".to_string())
    }
}

/// 获取 Cursor 可执行文件路径
pub fn get_cursor_executable_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let path = PathBuf::from("/Applications/Cursor.app/Contents/MacOS/Cursor");
        if path.exists() {
            return Ok(path);
        }
        Err("Cursor not found in /Applications".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        use std::env;

        let local_appdata = env::var("LOCALAPPDATA").ok();
        let program_files =
            env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());

        let mut possible_paths = Vec::new();

        if let Some(local) = local_appdata {
            possible_paths.push(
                PathBuf::from(&local)
                    .join("Programs")
                    .join("cursor")
                    .join("Cursor.exe"),
            );
        }

        possible_paths.push(
            PathBuf::from(&program_files)
                .join("Cursor")
                .join("Cursor.exe"),
        );

        for path in possible_paths {
            if path.exists() {
                return Ok(path);
            }
        }

        // 非默认目录安装：从卸载信息里的 InstallLocation 找
        if let Some(path) = find_cursor_from_registry() {
            return Ok(path);
        }

        // 最后兜底：正在运行的 Cursor 进程
        if let Some(path) = find_running_cursor_exe() {
            return Ok(path);
        }

        Err("Cursor not found (set a custom Cursor path in settings)".to_string())
    }

    #[cfg(target_os = "linux")]
    {
        let possible_paths = vec![
            PathBuf::from("/usr/bin/cursor"),
            PathBuf::from("/usr/local/bin/cursor"),
            PathBuf::from("/opt/Cursor/cursor"),
        ];

        for path in possible_paths {
            if path.exists() {
                return Ok(path);
            }
        }

        Err("Cursor not found".to_string())
    }
}

/// 从 Windows 卸载信息（HKCU / HKLM，含 WOW6432Node）读取 Cursor 的 InstallLocation
#[cfg(target_os = "windows")]
pub fn find_cursor_from_registry() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    const UNINSTALL_PATHS: [&str; 2] = [
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let root = RegKey::predef(hive);
        for uninstall in UNINSTALL_PATHS {
            let Ok(key) = root.open_subkey_with_flags(uninstall, KEY_READ) else {
                continue;
            };
            for name in key.enum_keys().flatten() {
                let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                let display_name: String = sub.get_value("DisplayName").unwrap_or_default();
                if !display_name.to_lowercase().starts_with("cursor") {
                    continue;
                }
                let location: String = sub.get_value("InstallLocation").unwrap_or_default();
                let location = location.trim().trim_matches('"');
                if location.is_empty() {
                    continue;
                }
                let exe = PathBuf::from(location).join("Cursor.exe");
                if exe.exists() {
                    return Some(exe);
                }
            }
        }
    }
    None
}

/// 找到正在运行的 Cursor 可执行文件路径（关闭 Cursor 之前调用，用于之后原路拉起）。
/// - Windows / Linux：返回可执行文件
/// - macOS：返回 .app 包路径（与自定义路径的格式一致）
pub fn find_running_cursor_exe() -> Option<PathBuf> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet),
    );

    for process in sys.processes().values() {
        if !is_cursor_process(process) {
            continue;
        }
        let Some(exe) = process.exe() else {
            continue;
        };

        #[cfg(target_os = "macos")]
        {
            let s = exe.to_string_lossy();
            if let Some(idx) = s.find(".app/") {
                let app = PathBuf::from(&s[..idx + 4]);
                if app.exists() {
                    return Some(app);
                }
            }
            continue;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let is_main_binary = exe
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| {
                    let n = n.to_lowercase();
                    n == "cursor.exe" || n == "cursor"
                })
                .unwrap_or(false);
            if is_main_binary && exe.exists() {
                return Some(exe.to_path_buf());
            }
        }
    }
    None
}

/// 解析本次要使用的 Cursor 路径：自定义路径 > 正在运行的进程 > 默认探测
pub fn resolve_cursor_path(custom_path: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = custom_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(p) = find_running_cursor_exe() {
        return Some(p);
    }
    get_cursor_executable_path().ok()
}

/// 让 Cursor 打开一个 `cursor://` 链接（Cursor 未运行时会先启动再处理）。
///
/// 注意：url 里带 token，任何错误信息和日志都不能包含它。
pub fn open_cursor_url(cursor_path: Option<&Path>, url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // macOS 由 LaunchServices 投递给已注册 cursor:// 的 Cursor.app
        let mut cmd = Command::new("open");
        if let Some(app) = cursor_path {
            cmd.arg("-a").arg(app);
        }
        let status = cmd
            .arg(url)
            .status()
            .map_err(|e| format!("Failed to open Cursor URL ({:?})", e.kind()))?;
        if !status.success() {
            return Err(format!("Failed to open Cursor URL (exit code {:?})", status.code()));
        }
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        // 与 Cursor 自己注册的协议处理命令一致：Cursor.exe --open-url -- "<url>"
        // 已有实例时，新进程只负责把参数转交给主进程然后退出
        if let Some(exe) = cursor_path.filter(|p| p.is_file()) {
            let mut cmd = Command::new(exe);
            cmd.args(["--open-url", "--", url]);
            match cmd.spawn() {
                Ok(_) => return Ok(()),
                Err(e) => eprintln!("Failed to pass URL via Cursor executable, falling back to shell open: {}", e),
            }
        }

        // 兜底：交给系统协议处理器（需要 cursor:// 已注册）
        // open 的错误信息会带上完整命令行（含 URL / token），这里只保留错误类型
        open::that(url)
            .map_err(|e| format!("Failed to open Cursor URL via system handler ({:?})", e.kind()))
    }
}

/// 启动 Cursor（支持自定义路径）
pub fn launch_cursor_with_path(custom_path: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let app_path = if let Some(path) = custom_path {
            PathBuf::from(path)
        } else {
            PathBuf::from("/Applications/Cursor.app")
        };

        if !app_path.exists() {
            return Err(format!("Cursor not found at {:?}", app_path));
        }

        Command::new("open")
            .arg("-a")
            .arg(app_path)
            .spawn()
            .map_err(|e| format!("Failed to launch Cursor: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        let exe_path = if let Some(path) = custom_path {
            let p = PathBuf::from(path);
            if !p.exists() {
                return Err(format!("Cursor not found at {:?}", p));
            }
            p
        } else {
            get_cursor_executable_path()?
        };

        Command::new(exe_path)
            .spawn()
            .map_err(|e| format!("Failed to launch Cursor: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let exe_path = if let Some(path) = custom_path {
            let p = PathBuf::from(path);
            if !p.exists() {
                return Err(format!("Cursor not found at {:?}", p));
            }
            p
        } else {
            get_cursor_executable_path()?
        };

        Command::new(exe_path)
            .spawn()
            .map_err(|e| format!("Failed to launch Cursor: {}", e))?;
    }

    Ok(())
}

/// 启动 Cursor（使用默认路径）
pub fn launch_cursor() -> Result<(), String> {
    launch_cursor_with_path(None)
}

/// 验证 Cursor 路径是否有效
pub fn validate_cursor_path(path: &str) -> Result<bool, String> {
    let path_buf = PathBuf::from(path);

    if !path_buf.exists() {
        return Ok(false);
    }

    #[cfg(target_os = "windows")]
    {
        let file_name = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        return Ok(file_name == "cursor.exe");
    }

    #[cfg(target_os = "macos")]
    {
        let path_str = path_buf.to_string_lossy().to_lowercase();
        return Ok(path_str.ends_with(".app") && path_str.contains("cursor"));
    }

    #[cfg(target_os = "linux")]
    {
        let file_name = path_buf.file_name().and_then(|n| n.to_str()).unwrap_or("");
        return Ok(file_name.eq_ignore_ascii_case("cursor"));
    }
}
