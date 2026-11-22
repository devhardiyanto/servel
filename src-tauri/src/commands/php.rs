//! Versi PHP lewat mise.
use serde::{Deserialize, Serialize};
use tauri::{Manager, Window};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PhpVersion {
    pub version: String,
    pub active: bool,
}

/// Versi terpasang. Tanpa mise: daftar kosong (onboarding menawarkan install).
#[tauri::command]
pub async fn php_list_installed() -> Result<Vec<PhpVersion>, String> {
    if !crate::mise::provider_is_mise().await {
        return Ok(Vec::new());
    }
    Ok(crate::mise::list_installed("php")
        .await?
        .into_iter()
        .map(|v| PhpVersion { version: v.version, active: v.active })
        .collect())
}

#[tauri::command]
pub async fn php_get_active() -> Result<Option<String>, String> {
    if !crate::mise::provider_is_mise().await {
        return Ok(None);
    }
    crate::mise::get_active("php").await
}

/// `scope`: `None` = otomatis, `"project"` = pin di project aktif,
/// `"global"` = ubah config global.
#[tauri::command]
pub async fn php_switch(window: Window, version: String, scope: Option<String>) -> Result<(), String> {
    crate::mise::require_mise().await?;
    crate::mise::validate_version(&version)?;
    crate::mise::use_version(window.app_handle(), "php", &version, scope.as_deref()).await
}

#[tauri::command]
pub async fn php_install(window: Window, version: String) -> Result<(), String> {
    crate::mise::require_mise().await?;
    crate::mise::validate_version(&version)?;
    crate::mise::install(window.app_handle(), "php", &version).await
}
