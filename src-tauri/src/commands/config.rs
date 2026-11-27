use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Preset pilihan service ber-nama. User bisa punya beberapa profil (mis. per
/// proyek) dan menukar set service aktif dengan meng-`apply` salah satunya.
/// `id` di-generate frontend (`crypto.randomUUID`); migrasi memakai id "default".
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub service_ids: Vec<String>,
}

/// Kelompok project di layar Projects. Urutan kelompok = urutan di `groups`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
}

/// Folder project yang dikelola Kova. Versi PHP/Node di project di-resolve
/// oleh mise dengan cwd = `path`. `id` di-generate frontend.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Folder project. Kosong hanya pada entri hasil migrasi site lama
    /// (domain `.test`) yang foldernya belum dipilih.
    #[serde(default)]
    pub path: String,
    /// Profil service yang diterapkan saat project ini dipilih.
    /// `None` = selection tidak diubah.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Jenis hasil deteksi (`laravel`, `vite`, ...). Hanya label tampilan.
    #[serde(default)]
    pub kind: String,
    /// Perintah dev server; `{port}` diganti port project saat dijalankan.
    /// Kosong = project hanya dipakai untuk versi/profil.
    #[serde(default)]
    pub command: String,
    /// Port tetap di `localhost`. 0 = belum ditetapkan.
    #[serde(default)]
    pub port: u16,
    /// Jalankan dev server otomatis saat Kova dibuka.
    #[serde(default)]
    pub auto_start: bool,
    /// Environment tambahan untuk dev server, dipasang di atas environment dari
    /// mise. `PORT` dikelola Kova dan tidak boleh diisi.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Environment lain selain Default: tiap entri hanya menyimpan nilai yang
    /// BERBEDA dari `env` atau yang tidak ada di sana. Kova versi lama
    /// mengabaikan field ini.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envs: Vec<ProjectEnv>,
    /// Nama environment yang dipakai saat start. `None` = Default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_env: Option<String>,
    /// Saat dev server dijalankan, nyalakan juga service di profil project.
    /// Tanpa `profile_id` tidak berpengaruh.
    #[serde(default)]
    pub start_services: bool,
    /// Kelompok di layar Projects. `None` (atau id yang sudah dihapus) =
    /// Ungrouped. Urutan project = urutan di `projects`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Windows, folder `\\wsl.localhost\<distro>\…`: jalankan command di
    /// dalam distro itu (lihat `wsl.rs`), bukan dengan tool Windows.
    #[serde(default)]
    pub wsl: bool,
    /// Cara menjalankan project: `command` (default), `dockerfile`, `compose`.
    #[serde(default, skip_serializing_if = "RunMode::is_command")]
    pub run_mode: RunMode,
    /// Setelan mode container (Dockerfile / compose).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerSpec>,
    /// Proses tambahan yang berjalan bersama dev server (queue worker,
    /// scheduler, Vite). Hanya untuk mode `command`. Kova versi lama
    /// mengabaikan field ini.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<ProjectProcess>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProcess {
    /// Nama pendek, unik per project (`queue`, `vite`). Dipakai sebagai
    /// prefiks baris log `[queue]`.
    pub name: String,
    pub command: String,
    /// Jalankan lagi bila berhenti sendiri.
    #[serde(default = "default_true")]
    pub auto_restart: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    #[default]
    Command,
    Dockerfile,
    Compose,
}

impl RunMode {
    pub fn is_command(&self) -> bool {
        *self == RunMode::Command
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSpec {
    /// Dockerfile relatif terhadap folder project. Kosong = `Dockerfile`.
    #[serde(default)]
    pub dockerfile: String,
    /// Port yang didengar app di dalam container. 0 = sama dengan port project.
    #[serde(default)]
    pub container_port: u16,
    /// Docker: gabung ke network service Kova (`servel_default`).
    #[serde(default = "crate::commands::config::default_true")]
    pub join_services: bool,
    /// File compose relatif terhadap folder project. Kosong = `compose.yaml`.
    #[serde(default)]
    pub compose_file: String,
    /// Service compose yang tidak dijalankan karena Kova sudah punya.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip_compose_services: Vec<String>,
}

/// Satu environment project selain Default.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnv {
    pub name: String,
    #[serde(default)]
    pub vars: std::collections::BTreeMap<String, String>,
}

impl Project {
    /// Environment aktif yang benar-benar ada. Nama yang sudah dihapus
    /// dianggap Default, supaya start tidak gagal karena config usang.
    pub fn active_env_entry(&self) -> Option<&ProjectEnv> {
        let name = self.active_env.as_deref()?;
        self.envs.iter().find(|e| e.name == name)
    }

    /// ENV yang dipasang saat start: Default, lalu override environment aktif.
    pub fn effective_env(&self) -> std::collections::BTreeMap<String, String> {
        let mut out = self.env.clone();
        if let Some(e) = self.active_env_entry() {
            for (k, v) in &e.vars {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

/// Target proxy site lama. Hanya dibaca saat migrasi ke v7.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SiteTarget {
    Port { value: u16 },
}

/// Site lama (domain `.test` lewat hosts + proxy). Fitur itu sudah dihapus;
/// struct ini hanya dibaca untuk memindahkan isinya ke `projects` dan tidak
/// pernah ditulis lagi.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LegacySite {
    pub id: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub target: Option<SiteTarget>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConfigState {
    #[serde(default)]
    pub version: u32,
    /// Proyeksi `service_ids` profil aktif. Tray dan CLI `kova up` membaca field ini,
    /// selalu di-sinkronkan dari profil aktif saat write (lihat `config_write`).
    #[serde(default)]
    pub selected_service_ids: Vec<String>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub active_profile_id: Option<String>,
    /// Site lama. Hanya dibaca; migrasi v7 memindahkannya ke `projects`.
    #[serde(default, skip_serializing)]
    pub sites: Vec<LegacySite>,
    #[serde(default)]
    pub last_php_version: Option<String>,
    #[serde(default)]
    pub last_node_version: Option<String>,
    #[serde(default)]
    pub watched_path: Option<String>,
    #[serde(default)]
    pub auto_start: bool,
    /// Start projects that have `auto_start` when Kova opens. Off = none of them.
    #[serde(default = "default_true")]
    pub auto_start_projects: bool,
    #[serde(default = "default_true")]
    pub remember_session: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Notifikasi Windows/desktop saat project, proses, atau service berhenti
    /// sendiri dan jendela Kova tidak sedang dilihat.
    #[serde(default = "default_true")]
    pub notifications: bool,
    /// Skala antarmuka: zoom webview, 0.9 / 1.0 / 1.1.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f64,
    /// Engine container: `auto` | `docker` | `wslc`. Lihat `crate::runtime::resolve`.
    /// Config lama tanpa field ini → `auto` (migrasi v4→v5).
    #[serde(default = "default_runtime")]
    pub container_runtime: String,
    /// Tidak dibaca lagi (mise satu-satunya pengelola versi); disimpan untuk
    /// kompatibilitas file config.
    #[serde(default = "default_runtime")]
    pub version_provider: String,
    #[serde(default)]
    pub projects: Vec<Project>,
    /// Kelompok project, dalam urutan tampil. Config lama tanpa field ini =
    /// tidak ada kelompok.
    #[serde(default)]
    pub groups: Vec<ProjectGroup>,
    /// Konteks versi di Dashboard. `None` = Global.
    #[serde(default)]
    pub active_project_id: Option<String>,
    /// Quit Kova juga menghentikan service container.
    #[serde(default = "default_true")]
    pub stop_services_on_quit: bool,
    /// Onboarding sudah dilewati sekali; Kova langsung membuka Projects.
    #[serde(default)]
    pub onboarded: bool,
    /// Port di PC per service (Services › Change port). Hanya port pertama
    /// service; port di dalam container tidak berubah.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub service_ports: std::collections::BTreeMap<String, u16>,
    /// Services › Add service. Dirapikan saat dimuat (`sanitize_custom`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_services: Vec<crate::commands::services::ServiceDef>,
}

pub fn default_true() -> bool {
    true
}

fn default_ui_scale() -> f64 {
    1.0
}

fn default_runtime() -> String {
    "auto".to_string()
}

/// Migrasikan config lama ke skema terkini (`CURRENT_VERSION`).
///
/// - v0 → v1: `version` absent (serde default 0) → bump ke 1.
/// - v1 → v2: config flat (`selected_service_ids`, tanpa `profiles`) → buat
///   profil "Default" berisi selection existing, set `active_profile_id`.
/// - v2 → v3: perkenalkan `sites`. Config lama tanpa field
///   ini di-isi `[]` oleh serde default; cukup bump versi.
/// - v3 → v4: perkenalkan `Site.target`. Site lama tak
///   punya makna proxy, jadi sengaja **tidak** diberi target default; serde
///   mengisinya `None` dan site tetap hosts-only. Cukup bump versi.
/// - v4 → v5: perkenalkan `container_runtime` (WSL containers). Serde
///   default `auto` → install lama di Windows yang masih punya Docker tetap di
///   Docker; pindah ke WSLC dilakukan user lewat Settings.
/// - v5 → v6: perkenalkan `projects`, `active_project_id`, `version_provider`
///   (mise). `watched_path` lama (bila ada) dijadikan project pertama.
/// - v6 → v7: project bisa dijalankan di `localhost:port` dan fitur
///   domain `.test` dihapus. Site lama dipindah ke `projects`: domain menjadi
///   nama, port target menjadi port, folder masih kosong. File hosts tidak
///   disentuh di sini.
///
/// Aman dipanggil berulang; invariant diperbaiki di setiap baca.
fn migrate(mut cfg: ConfigState) -> ConfigState {
    // Pengguna lama yang sudah memakai Kova tidak perlu onboarding lagi.
    if !cfg.onboarded && (!cfg.projects.is_empty() || cfg.profiles.iter().any(|p| !p.service_ids.is_empty())) {
        cfg.onboarded = true;
    }
    if cfg.version == 0 {
        cfg.version = 1;
    }

    // v1 → v2: perkenalkan profiles. Config lama tak punya profil sama sekali.
    if cfg.profiles.is_empty() {
        cfg.profiles = vec![Profile {
            id: DEFAULT_PROFILE_ID.to_string(),
            name: "Default".to_string(),
            service_ids: cfg.selected_service_ids.clone(),
        }];
        cfg.active_profile_id = Some(DEFAULT_PROFILE_ID.to_string());
    }

    // Jaga invariant: active_profile_id harus menunjuk profil yang ada.
    let active_valid = cfg
        .active_profile_id
        .as_ref()
        .is_some_and(|id| cfg.profiles.iter().any(|p| &p.id == id));
    if !active_valid {
        cfg.active_profile_id = cfg.profiles.first().map(|p| p.id.clone());
    }

    // v5 → v6: folder yang dulu di-watch menjadi project pertama.
    if cfg.version < 6 && cfg.projects.is_empty() {
        if let Some(path) = cfg.watched_path.clone().filter(|p| !p.trim().is_empty()) {
            cfg.projects.push(Project {
                id: "watched".to_string(),
                name: project_name_from_path(&path),
                path,
                profile_id: None,
                kind: String::new(),
                command: String::new(),
                port: 0,
                auto_start: false,
                env: Default::default(),
                envs: Vec::new(),
                active_env: None,
                start_services: false,
                group_id: None,
                wsl: false,
                run_mode: Default::default(),
                container: None,
                processes: Vec::new(),
            });
        }
    }
    // v6 → v7: site lama menjadi project (folder belum dipilih).
    for site in std::mem::take(&mut cfg.sites) {
        if cfg.projects.iter().any(|p| p.id == site.id) {
            continue;
        }
        let port = match site.target {
            Some(SiteTarget::Port { value }) => value,
            None => 0,
        };
        cfg.projects.push(Project {
            id: site.id,
            name: site.domain,
            path: String::new(),
            profile_id: None,
            kind: String::new(),
            command: String::new(),
            port,
            auto_start: false,
            env: Default::default(),
            envs: Vec::new(),
            active_env: None,
            start_services: false,
            group_id: None,
            wsl: false,
            run_mode: Default::default(),
            container: None,
            processes: Vec::new(),
        });
    }

    // Service yang dihapus dari services.json dibuang dari profil, supaya Start
    // tidak mencoba service yang sudah tidak ada. Idempotent.
    for p in cfg.profiles.iter_mut() {
        p.service_ids.retain(|id| !REMOVED_SERVICE_IDS.contains(&id.as_str()));
    }
    cfg.selected_service_ids
        .retain(|id| !REMOVED_SERVICE_IDS.contains(&id.as_str()));

    // Invariant: profil yang ditautkan ke project harus ada.
    let profile_ids: Vec<String> = cfg.profiles.iter().map(|p| p.id.clone()).collect();
    for p in cfg.projects.iter_mut() {
        if p.profile_id.as_ref().is_some_and(|id| !profile_ids.contains(id)) {
            p.profile_id = None;
        }
    }

    // Invariant: kelompok project harus ada; yang sudah dihapus = Ungrouped.
    for p in cfg.projects.iter_mut() {
        if p.group_id.as_ref().is_some_and(|id| !cfg.groups.iter().any(|g| &g.id == id)) {
            p.group_id = None;
        }
    }

    // Invariant: active_project_id menunjuk project yang ada (atau None).
    if let Some(id) = cfg.active_project_id.clone() {
        if !cfg.projects.iter().any(|p| p.id == id) {
            cfg.active_project_id = None;
        }
    }

    cfg.version = CURRENT_VERSION;
    project_active_selection(&mut cfg);
    cfg
}

/// Nama tampilan project = nama folder terakhir.
fn project_name_from_path(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Sinkronkan `selected_service_ids` = `service_ids` profil aktif. Menjamin tray
/// dan CLI `kova up` (yang membaca `selected_service_ids`) selalu match profil aktif.
fn project_active_selection(cfg: &mut ConfigState) {
    if let Some(active_id) = cfg.active_profile_id.clone() {
        if let Some(p) = cfg.profiles.iter().find(|p| p.id == active_id) {
            cfg.selected_service_ids = p.service_ids.clone();
        }
    }
}

/// Service yang pernah ada lalu dihapus dari services.json.
/// - `sqlserver`: dihapus (image ~1.5 GB, jarang dipakai).
const REMOVED_SERVICE_IDS: &[&str] = &["sqlserver"];

/// Id profil bawaan yang dibuat saat migrasi/instal baru.
const DEFAULT_PROFILE_ID: &str = "default";

/// Skema config terkini. Naikkan saat ada perubahan struktur yang butuh migrasi.
const CURRENT_VERSION: u32 = 7;

impl Default for ConfigState {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            selected_service_ids: Vec::new(),
            profiles: vec![Profile {
                id: DEFAULT_PROFILE_ID.to_string(),
                name: "Default".to_string(),
                service_ids: Vec::new(),
            }],
            active_profile_id: Some(DEFAULT_PROFILE_ID.to_string()),
            sites: Vec::new(),
            last_php_version: None,
            last_node_version: None,
            watched_path: None,
            auto_start: false,
            auto_start_projects: true,
            remember_session: true,
            minimize_to_tray: true,
            notifications: true,
            ui_scale: 1.0,
            container_runtime: default_runtime(),
            version_provider: default_runtime(),
            projects: Vec::new(),
            groups: Vec::new(),
            active_project_id: None,
            stop_services_on_quit: true,
            onboarded: false,
            service_ports: Default::default(),
            custom_services: Vec::new(),
        }
    }
}

/// Tulis config ke `path` secara atomik + durable terhadap hard restart PC.
///
/// Urutan: tulis ke `.tmp` → `sync_all()` (fsync isi file ke disk) → `rename` →
/// fsync direktori parent (best-effort) agar entri rename ikut durable.
/// Tanpa fsync file, pada hard power-loss isi bisa belum ter-flush walau rename
/// sudah ter-journal → config.json kosong/truncated.
fn write_config_atomic(path: &Path, json: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create the config folder: {}", e))?;
    }

    // Satu penulis sekaligus: config_write async bisa jalan paralel dan
    // berbagi nama file sementara.
    static WRITE: Mutex<()> = Mutex::new(());
    let _turn = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let tmp_path = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("could not create config.tmp: {}", e))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("could not write config.tmp: {}", e))?;
        f.sync_all()
            .map_err(|e| format!("could not flush config.tmp: {}", e))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("could not rename config.tmp to config.json: {}", e))?;

    // fsync folder supaya rename durable; di Windows bisa gagal, diabaikan.
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Baca + migrasi config.json secara sinkron (tanpa sentuh Mutex in-memory).
/// Dipakai command lain (mis. `sites_*`) yang butuh state config terkini.
/// Config absent/corrupt → `ConfigState::default()`; yang corrupt disalin dulu (sama seperti `config_read`).
pub fn load_config(app: &AppHandle) -> Result<ConfigState, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(ConfigState::default());
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Ok(ConfigState::default()),
    };
    match serde_json::from_str::<ConfigState>(&raw) {
        Ok(cfg) => Ok(migrate(cfg)),
        Err(_) => {
            keep_corrupt_copy(&path, &raw);
            Ok(ConfigState::default())
        }
    }
}

/// Simpan salinan config.json yang tidak terbaca sebelum default menimpanya
/// pada penulisan berikutnya. Isi yang sama tidak disalin dua kali.
fn keep_corrupt_copy(path: &Path, raw: &str) -> Option<PathBuf> {
    let dir = path.parent()?;
    let exists = std::fs::read_dir(dir).ok()?.flatten().any(|e| {
        e.file_name().to_string_lossy().starts_with("config.json.corrupt-")
            && std::fs::read_to_string(e.path()).is_ok_and(|c| c == raw)
    });
    if exists {
        return None;
    }
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let copy = dir.join(format!("config.json.corrupt-{ts}"));
    std::fs::write(&copy, raw).ok()?;
    Some(copy)
}

pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("could not find the app config folder: {}", e))?;
    Ok(config_dir.join("config.json"))
}

#[tauri::command]
pub async fn config_read(app: AppHandle) -> Result<ConfigState, String> {
    let path = config_path(&app)?;

    if !path.exists() {
        return Ok(ConfigState::default());
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read config.json: {}", e));

    let raw = match raw {
        Ok(s) => s,
        Err(err) => {
            eprintln!("[config_read] warning: {err}, returning default");
            return Ok(ConfigState::default());
        }
    };

    let cfg: ConfigState = match serde_json::from_str(&raw) {
        Ok(c) => c,
        Err(err) => {
            let copy = keep_corrupt_copy(&path, &raw);
            eprintln!("[config_read] warning: config.json corrupt ({err}), returning default; copy: {copy:?}");
            return Ok(ConfigState::default());
        }
    };

    // Migrasi ke skema terkini. Config lama tetap kebaca lewat serde
    // default lalu dinaikkan; selection lama masuk ke profil "Default".
    let cfg = migrate(cfg);

    crate::runtime::apply_preference(&cfg.container_runtime).await;

    // Sync ke in-memory Mutex agar tray (yang baca Mutex) lihat selection tersimpan
    // tanpa harus menunggu user toggle ulang setelah app boot.
    if let Some(state) = app.try_state::<Mutex<ConfigState>>() {
        if let Ok(mut guard) = state.lock() {
            *guard = cfg.clone();
        }
    }

    Ok(cfg)
}

#[tauri::command]
pub async fn config_write(app: AppHandle, mut config: ConfigState) -> Result<(), String> {
    let path = config_path(&app)?;

    // Invariant: selected_service_ids selalu = profil aktif (proyeksi). Tray dan
    // CLI membaca field ini; frontend cukup update profil, backend proyeksi.
    project_active_selection(&mut config);

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("could not serialize the config: {}", e))?;

    // Atomic + durable write: tulis ke .tmp, fsync, lalu rename.
    write_config_atomic(&path, &json)?;

    // Preferensi engine bisa berubah dari Settings → resolve ulang sekarang,
    // supaya polling berikutnya langsung memakai engine baru.
    crate::runtime::apply_preference(&config.container_runtime).await;

    // Tray membaca Mutex ini, bukan disk.
    if let Some(state) = app.try_state::<Mutex<ConfigState>>() {
        if let Ok(mut guard) = state.lock() {
            *guard = config.clone();
        }
    }

    // Nama, port, dan daftar project tampil di menu tray.
    crate::tray::refresh(&app);

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn corrupt_config_is_copied_once() {
        let dir = std::env::temp_dir().join(format!("kova-cfg-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ broken").unwrap();
        let copy = super::keep_corrupt_copy(&path, "{ broken").expect("copy");
        assert_eq!(std::fs::read_to_string(copy).unwrap(), "{ broken");
        assert!(super::keep_corrupt_copy(&path, "{ broken").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn effective_env_layers_active_environment_over_default() {
        let mut p = Project {
            id: "p".into(),
            name: "billing-worker".into(),
            env: [("APP_ENV".to_string(), "local".to_string()), ("DATABASE_URL".to_string(), "local_db".to_string())].into(),
            envs: vec![ProjectEnv {
                name: "tenant-acme".into(),
                vars: [("DATABASE_URL".to_string(), "acme_db".to_string()), ("TENANT_ID".to_string(), "acme".to_string())].into(),
            }],
            ..Default::default()
        };
        assert_eq!(p.effective_env(), p.env);
        p.active_env = Some("tenant-acme".into());
        let e = p.effective_env();
        assert_eq!(e["APP_ENV"], "local");
        assert_eq!(e["DATABASE_URL"], "acme_db");
        assert_eq!(e["TENANT_ID"], "acme");
        // Environment yang sudah dihapus = Default.
        p.active_env = Some("gone".into());
        assert_eq!(p.effective_env(), p.env);
        assert!(p.active_env_entry().is_none());
    }

    #[test]
    fn project_without_envs_serializes_like_before() {
        let p = Project { id: "p".into(), name: "x".into(), ..Default::default() };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v.get("envs").is_none() && v.get("activeEnv").is_none() && v.get("groupId").is_none());
        let back: Project = serde_json::from_value(serde_json::json!({ "id": "p", "name": "x", "activeEnv": "t", "envs": [{ "name": "t", "vars": { "A": "1" } }] })).unwrap();
        assert_eq!(back.envs[0].vars["A"], "1");
        assert_eq!(back.active_env.as_deref(), Some("t"));
    }

    #[test]
    fn existing_users_count_as_onboarded() {
        let fresh = migrate(ConfigState::default());
        assert!(!fresh.onboarded, "a brand-new install still sees onboarding");
        assert!(fresh.stop_services_on_quit);
        let mut used = ConfigState::default();
        used.profiles[0].service_ids = vec!["mysql".into()];
        assert!(migrate(used).onboarded);
        // Field lama tidak ada di JSON: default dipakai.
        let old: ConfigState = serde_json::from_str(r#"{"version":7}"#).unwrap();
        assert!(old.stop_services_on_quit && !old.onboarded);
    }

    #[test]
    fn test_default_config_shape() {
        let cfg = ConfigState::default();
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert!(cfg.sites.is_empty());
        assert!(!cfg.auto_start);
        assert!(cfg.remember_session);
        assert!(cfg.minimize_to_tray);
        assert!(cfg.selected_service_ids.is_empty());
        assert!(cfg.last_php_version.is_none());
        assert!(cfg.last_node_version.is_none());
        assert!(cfg.watched_path.is_none());
        // Instal baru: tepat 1 profil "Default" yang aktif.
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].id, "default");
        assert_eq!(cfg.profiles[0].name, "Default");
        assert_eq!(cfg.active_profile_id.as_deref(), Some("default"));
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original = ConfigState {
            version: 2,
            selected_service_ids: vec!["mysql".to_string(), "redis".to_string()],
            profiles: vec![Profile {
                id: "default".to_string(),
                name: "Default".to_string(),
                service_ids: vec!["mysql".to_string(), "redis".to_string()],
            }],
            active_profile_id: Some("default".to_string()),
            sites: Vec::new(),
            last_php_version: Some("8.3".to_string()),
            last_node_version: Some("20.0.0".to_string()),
            watched_path: Some("/home/user/project".to_string()),
            auto_start: true,
            auto_start_projects: true,
            remember_session: false,
            minimize_to_tray: true,
            notifications: true,
            ui_scale: 1.0,
            container_runtime: "auto".to_string(),
            version_provider: "auto".to_string(),
            projects: Vec::new(),
            groups: Vec::new(),
            active_project_id: None,
            stop_services_on_quit: true,
            onboarded: false,
            service_ports: Default::default(),
            custom_services: Vec::new(),
        };

        let json = serde_json::to_string(&original).expect("serialize gagal");

        // Verifikasi camelCase keys ada di JSON output
        assert!(json.contains("selectedServiceIds"));
        assert!(json.contains("lastPhpVersion"));
        assert!(json.contains("lastNodeVersion"));
        assert!(json.contains("watchedPath"));
        assert!(json.contains("autoStart"));
        assert!(json.contains("rememberSession"));
        assert!(json.contains("minimizeToTray"));
        assert!(json.contains("profiles"));
        assert!(json.contains("activeProfileId"));
        assert!(json.contains("serviceIds"));

        let restored: ConfigState = serde_json::from_str(&json).expect("deserialize gagal");
        assert_eq!(restored.version, original.version);
        assert_eq!(restored.selected_service_ids, original.selected_service_ids);
        assert_eq!(restored.profiles.len(), 1);
        assert_eq!(restored.profiles[0].service_ids, original.profiles[0].service_ids);
        assert_eq!(restored.active_profile_id, original.active_profile_id);
        // Site lama tidak pernah ditulis lagi.
        assert!(!json.contains("\"sites\""));
        assert_eq!(restored.last_php_version, original.last_php_version);
        assert_eq!(restored.last_node_version, original.last_node_version);
        assert_eq!(restored.watched_path, original.watched_path);
        assert_eq!(restored.auto_start, original.auto_start);
        assert_eq!(restored.remember_session, original.remember_session);
        assert_eq!(restored.minimize_to_tray, original.minimize_to_tray);
    }

    #[test]
    fn test_parse_with_unknown_field() {
        // Field tak dikenal dilewati, bukan error.
        let json = r#"{
            "version": 1,
            "selectedServiceIds": ["mysql"],
            "autoStart": false,
            "rememberSession": true,
            "minimizeToTray": true,
            "unknownFutureField": "some_value",
            "anotherUnknown": 42
        }"#;

        let cfg: ConfigState = serde_json::from_str(json).expect("harus berhasil parse");
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.selected_service_ids, vec!["mysql"]);
        assert!(!cfg.auto_start);
        assert!(cfg.remember_session);
        assert!(cfg.minimize_to_tray);
    }

    #[test]
    fn test_write_config_atomic_roundtrip() {
        // Tulis ke tempdir lalu baca ulang & verifikasi isi durable + benar.
        let mut dir = std::env::temp_dir();
        dir.push(format!("servel_test_{}", std::process::id()));
        let path = dir.join("config.json");

        let cfg = ConfigState {
            version: 2,
            selected_service_ids: vec!["mysql".to_string(), "minio".to_string()],
            profiles: vec![Profile {
                id: "default".to_string(),
                name: "Default".to_string(),
                service_ids: vec!["mysql".to_string(), "minio".to_string()],
            }],
            active_profile_id: Some("default".to_string()),
            sites: Vec::new(),
            last_php_version: Some("8.3".to_string()),
            last_node_version: None,
            watched_path: Some("/tmp/proj".to_string()),
            auto_start: true,
            auto_start_projects: true,
            remember_session: true,
            minimize_to_tray: false,
            notifications: true,
            ui_scale: 1.0,
            container_runtime: "wslc".to_string(),
            version_provider: "mise".to_string(),
            projects: vec![Project {
                id: "p1".into(),
                name: "acme-api".into(),
                path: r"C:\dev\acme\acme-api".into(),
                profile_id: Some("default".into()),
                kind: "laravel".into(),
                command: "php artisan serve --host=127.0.0.1 --port={port}".into(),
                port: 8001,
                auto_start: true,
                env: [("SSL_CERT_FILE".to_string(), r"C:\ca.crt".to_string())].into(),
                envs: Vec::new(),
                active_env: None,
                start_services: true,
                group_id: None,
                wsl: false,
                run_mode: Default::default(),
                container: None,
                processes: Vec::new(),
            }],
            groups: Vec::new(),
            active_project_id: Some("p1".into()),
            stop_services_on_quit: true,
            onboarded: true,
            service_ports: Default::default(),
            custom_services: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&cfg).expect("serialize");

        write_config_atomic(&path, &json).expect("write_config_atomic gagal");

        let raw = std::fs::read_to_string(&path).expect("baca ulang gagal");
        let restored: ConfigState = serde_json::from_str(&raw).expect("parse ulang gagal");
        assert_eq!(restored.selected_service_ids, cfg.selected_service_ids);
        assert_eq!(restored.last_php_version, cfg.last_php_version);
        assert_eq!(restored.watched_path, cfg.watched_path);
        assert!(restored.auto_start);
        assert!(!restored.minimize_to_tray);

        // tmp tidak boleh tersisa setelah rename.
        assert!(!path.with_extension("tmp").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migrate_v1_to_v2_builds_default_profile() {
        // Config lama (v1: selectedServiceIds, tanpa profiles) → migrasi buat
        // profil "Default" berisi selection existing + set active_profile_id.
        let json = r#"{
            "version": 1,
            "selectedServiceIds": ["postgres", "redis"],
            "autoStart": false,
            "rememberSession": true,
            "minimizeToTray": false
        }"#;

        let cfg: ConfigState = serde_json::from_str(json).expect("parse gagal");
        let cfg = migrate(cfg);

        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].id, "default");
        assert_eq!(cfg.profiles[0].name, "Default");
        assert_eq!(cfg.profiles[0].service_ids, vec!["postgres", "redis"]);
        assert_eq!(cfg.active_profile_id.as_deref(), Some("default"));
        // selected_service_ids tetap = proyeksi profil aktif.
        assert_eq!(cfg.selected_service_ids, vec!["postgres", "redis"]);
        assert!(!cfg.minimize_to_tray);
    }

    #[test]
    fn test_migrate_v0_absent_version() {
        // JSON tanpa "version" → serde default 0 → naik ke skema terkini + profil Default.
        let json = r#"{ "selectedServiceIds": ["mysql"] }"#;
        let cfg: ConfigState = serde_json::from_str(json).expect("parse gagal");
        let cfg = migrate(cfg);
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.active_profile_id.as_deref(), Some("default"));
        assert_eq!(cfg.profiles[0].service_ids, vec!["mysql"]);
    }

    #[test]
    fn test_migrate_v7_moves_legacy_sites_into_projects() {
        // Site lama: domain + ip + enabled (+ target). Domain menjadi nama
        // project, port target dipertahankan, folder dan perintah masih kosong.
        let json = r#"{
            "version": 6,
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": [] }],
            "activeProfileId": "default",
            "projects": [{ "id": "p1", "name": "acme-api", "path": "C:\\dev\\acme\\acme-api", "profileId": "default" }],
            "sites": [
                { "id": "s-1", "domain": "myapp.test", "ip": "127.0.0.1", "enabled": true },
                { "id": "s-2", "domain": "shop-api.test", "ip": "127.0.0.1", "enabled": false,
                  "target": { "kind": "port", "value": 8001 } }
            ]
        }"#;

        let cfg = migrate(serde_json::from_str::<ConfigState>(json).expect("parse gagal"));

        assert_eq!(cfg.version, CURRENT_VERSION);
        assert!(cfg.sites.is_empty());
        assert_eq!(cfg.projects.len(), 3);
        // Project lama utuh, tanpa perintah/port.
        assert_eq!(cfg.projects[0].name, "acme-api");
        assert_eq!(cfg.projects[0].profile_id.as_deref(), Some("default"));
        assert_eq!((cfg.projects[0].command.as_str(), cfg.projects[0].port), ("", 0));
        assert_eq!((cfg.projects[1].name.as_str(), cfg.projects[1].port), ("myapp.test", 0));
        assert_eq!((cfg.projects[2].name.as_str(), cfg.projects[2].port), ("shop-api.test", 8001));
        assert!(cfg.projects[1..].iter().all(|p| p.path.is_empty() && p.command.is_empty()));

        // Setelah disimpan dan dibaca lagi, tidak ada duplikasi.
        let saved = serde_json::to_string(&cfg).unwrap();
        let again = migrate(serde_json::from_str::<ConfigState>(&saved).unwrap());
        assert_eq!(again.projects.len(), 3);
    }

    #[test]
    fn test_migrate_v4_to_v5_defaults_runtime_auto() {
        // Config skema v4 belum punya `containerRuntime` → `auto`, sisanya utuh.
        let json = r#"{
            "version": 4,
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": ["mysql"] }],
            "activeProfileId": "default",
            "autoStart": true
        }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).expect("parse gagal"));
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.container_runtime, "auto");
        assert!(cfg.auto_start);
        assert_eq!(cfg.selected_service_ids, vec!["mysql".to_string()]);
    }

    #[test]
    fn test_migrate_v5_to_v6_watched_path_becomes_project() {
        let json = r#"{
            "version": 5,
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": [] }],
            "activeProfileId": "default",
            "watchedPath": "C:\\dev\\acme\\",
            "containerRuntime": "wslc"
        }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).expect("parse gagal"));
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.version_provider, "auto");
        assert_eq!(cfg.projects.len(), 1);
        assert_eq!(cfg.projects[0].name, "acme");
        assert_eq!(cfg.projects[0].path, r"C:\dev\acme\");
        assert!(cfg.active_project_id.is_none());
        assert_eq!(cfg.container_runtime, "wslc");
    }

    #[test]
    fn test_migrate_drops_removed_services() {
        let json = r#"{
            "version": 6,
            "selectedServiceIds": ["mysql", "sqlserver"],
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": ["sqlserver", "redis"] }],
            "activeProfileId": "default"
        }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).unwrap());
        assert_eq!(cfg.profiles[0].service_ids, vec!["redis".to_string()]);
        // selected diproyeksikan ulang dari profil aktif.
        assert_eq!(cfg.selected_service_ids, vec!["redis".to_string()]);
    }

    #[test]
    fn test_migrate_drops_dangling_project_profile() {
        let json = r#"{
            "version": 6,
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": [] }],
            "activeProfileId": "default",
            "projects": [
                { "id": "a", "name": "a", "path": "/a", "profileId": "default" },
                { "id": "b", "name": "b", "path": "/b", "profileId": "deleted" }
            ]
        }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).unwrap());
        assert_eq!(cfg.projects[0].profile_id.as_deref(), Some("default"));
        assert_eq!(cfg.projects[1].profile_id, None);
    }

    #[test]
    fn groups_load_from_old_configs_and_dangling_group_ids_become_ungrouped() {
        let old: ConfigState = serde_json::from_str(r#"{"version":7,"projects":[{"id":"a","name":"acme-api"}]}"#).unwrap();
        assert!(old.groups.is_empty());
        assert_eq!(old.projects[0].group_id, None);

        let json = r#"{
            "version": 7,
            "groups": [{ "id": "g1", "name": "Acme" }, { "id": "g2", "name": "Shop", "collapsed": true }],
            "projects": [
                { "id": "a", "name": "acme-api", "groupId": "g1" },
                { "id": "b", "name": "shop-web", "groupId": "gone" }
            ]
        }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).unwrap());
        assert_eq!(cfg.groups.len(), 2);
        assert!(!cfg.groups[0].collapsed && cfg.groups[1].collapsed);
        assert_eq!(cfg.projects[0].group_id.as_deref(), Some("g1"));
        assert_eq!(cfg.projects[1].group_id, None);
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(v["projects"][0]["groupId"], "g1");
        assert_eq!(v["groups"][1]["collapsed"], true);
    }

    #[test]
    fn test_migrate_drops_dangling_active_project() {
        let json = r#"{ "version": 6, "projects": [], "activeProjectId": "gone" }"#;
        let cfg = migrate(serde_json::from_str::<ConfigState>(json).unwrap());
        assert!(cfg.active_project_id.is_none());
    }

    #[test]
    fn test_project_name_from_path() {
        assert_eq!(project_name_from_path(r"C:\dev\acme\acme-web"), "acme-web");
        assert_eq!(project_name_from_path("/home/u/shop/"), "shop");
    }

    #[test]
    fn test_container_runtime_roundtrip_camel_case() {
        let cfg = ConfigState {
            container_runtime: "wslc".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"containerRuntime\":\"wslc\""));
        let back: ConfigState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.container_runtime, "wslc");
    }

    #[test]
    fn test_migrate_v2_idempotent() {
        // Config v2 dgn profil custom aktif tidak boleh diubah/ditimpa.
        let original = ConfigState {
            version: 2,
            selected_service_ids: vec!["mongodb".to_string()],
            profiles: vec![
                Profile {
                    id: "p-abc".to_string(),
                    name: "Kerja".to_string(),
                    service_ids: vec!["mongodb".to_string()],
                },
                Profile {
                    id: "p-xyz".to_string(),
                    name: "Sampingan".to_string(),
                    service_ids: vec!["mysql".to_string(), "minio".to_string()],
                },
            ],
            active_profile_id: Some("p-abc".to_string()),
            ..Default::default()
        };

        let migrated = migrate(original.clone());
        assert_eq!(migrated.version, CURRENT_VERSION);
        assert_eq!(migrated.profiles.len(), 2);
        assert_eq!(migrated.active_profile_id.as_deref(), Some("p-abc"));
        assert_eq!(migrated.selected_service_ids, vec!["mongodb"]);
    }

    #[test]
    fn test_migrate_invalid_active_falls_back_to_first() {
        // active_profile_id menunjuk profil yang tak ada → fallback ke profil pertama.
        let cfg = ConfigState {
            version: 2,
            profiles: vec![Profile {
                id: "p-1".to_string(),
                name: "Satu".to_string(),
                service_ids: vec!["redis".to_string()],
            }],
            active_profile_id: Some("hilang".to_string()),
            ..Default::default()
        };
        let cfg = migrate(cfg);
        assert_eq!(cfg.active_profile_id.as_deref(), Some("p-1"));
        assert_eq!(cfg.selected_service_ids, vec!["redis"]);
    }

    #[test]
    fn test_migrate_v2_to_v3_adds_empty_sites() {
        // Config v2 (punya profiles, tanpa field `sites`) → migrasi isi sites=[]
        // via serde default + bump versi ke 3. Profil existing tak berubah.
        let json = r#"{
            "version": 2,
            "selectedServiceIds": ["mysql"],
            "profiles": [{ "id": "default", "name": "Default", "serviceIds": ["mysql"] }],
            "activeProfileId": "default",
            "autoStart": false,
            "rememberSession": true,
            "minimizeToTray": true
        }"#;
        let cfg: ConfigState = serde_json::from_str(json).expect("parse gagal");
        assert!(cfg.sites.is_empty());
        let cfg = migrate(cfg);
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert!(cfg.sites.is_empty());
        // Profil existing dipertahankan (migrasi v2→v3 tak menyentuh profiles).
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.active_profile_id.as_deref(), Some("default"));
    }

    #[test]
    fn test_empty_config_gets_default_profile() {
        // Config kosong total → default 1 profil aktif.
        let cfg: ConfigState = serde_json::from_str("{}").expect("parse gagal");
        let cfg = migrate(cfg);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.active_profile_id.as_deref(), Some("default"));
    }
}
