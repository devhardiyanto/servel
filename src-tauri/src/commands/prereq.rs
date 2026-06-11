use serde::{Deserialize, Serialize};
use tokio::process::Command;
use std::process::Stdio;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct PrereqStatus {
    pub docker_installed: bool,
    pub docker_running: bool,
    pub phpvm_installed: bool,
    pub fnm_installed: bool,
}

/// Spawn a command and return true if it exits successfully.
/// On Windows, uses CREATE_NO_WINDOW to prevent terminal flicker.
async fn check_tool(cmd: &str, args: &[&str]) -> bool {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let result = Command::new(cmd)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;

        match result {
            Ok(status) => status.success(),
            // On Windows some tools (phpvm.cmd, fnm) need shell wrapping — retry via cmd /c
            Err(_) => {
                let shell_result = Command::new("cmd")
                    .args(["/c", cmd])
                    .args(args)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .status()
                    .await;
                shell_result.map(|s| s.success()).unwrap_or(false)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        silent_command(cmd)
            .args(args)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[tauri::command]
pub async fn check_prerequisites() -> PrereqStatus {
    let (docker_installed, docker_running, phpvm_installed, fnm_installed) = tokio::join!(
        check_tool("docker", &["--version"]),
        check_tool("docker", &["info"]),
        check_tool("phpvm", &["--version"]),
        check_tool("fnm", &["--version"]),
    );

    PrereqStatus {
        docker_installed,
        docker_running,
        phpvm_installed,
        fnm_installed,
    }
}

#[tauri::command]
pub async fn start_docker() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let default_path = "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe";

        if std::path::Path::new(default_path).exists() {
            Command::new(default_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Gagal menjalankan Docker Desktop: {}", e))?;
        } else {
            // Fallback via Start Menu shortcut
            Command::new("cmd")
                .args(["/c", "start", "", "Docker Desktop"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Gagal menjalankan Docker Desktop via Start Menu: {}", e))?;
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Docker"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Gagal membuka Docker Desktop: {}", e))?;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        Err("Docker daemon harus distart manual via systemctl start docker.".to_string())
    }
}
