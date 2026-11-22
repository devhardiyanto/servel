//! Versi Node lewat mise.
use serde::{Deserialize, Serialize};
use tauri::{Manager, Window};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NodeVersion {
    pub version: String,
    pub active: bool,
}

/// Versi terpasang. Tanpa mise: daftar kosong (onboarding menawarkan install).
#[tauri::command]
pub async fn node_list_installed() -> Result<Vec<NodeVersion>, String> {
    if !crate::mise::provider_is_mise().await {
        return Ok(Vec::new());
    }
    Ok(crate::mise::list_installed("node")
        .await?
        .into_iter()
        .map(|v| NodeVersion { version: v.version, active: v.active })
        .collect())
}

#[tauri::command]
pub async fn node_get_active() -> Result<Option<String>, String> {
    if !crate::mise::provider_is_mise().await {
        return Ok(None);
    }
    crate::mise::get_active("node").await
}

/// `scope`: `None` = otomatis, `"project"` = pin di project aktif,
/// `"global"` = ubah config global.
#[tauri::command]
pub async fn node_switch(window: Window, version: String, scope: Option<String>) -> Result<(), String> {
    crate::mise::require_mise().await?;
    crate::mise::validate_version(&version)?;
    crate::mise::use_version(window.app_handle(), "node", &version, scope.as_deref()).await
}

#[tauri::command]
pub async fn node_install(window: Window, version: String) -> Result<(), String> {
    crate::mise::require_mise().await?;
    crate::mise::validate_version(&version)?;
    crate::mise::install(window.app_handle(), "node", &version).await
}
