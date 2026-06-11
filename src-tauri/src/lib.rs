mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::prereq::check_prerequisites,
            commands::prereq::start_docker,
            commands::php::php_list_installed,
            commands::php::php_get_active,
            commands::php::php_switch,
            commands::php::php_install,
            commands::node::node_list_installed,
            commands::node::node_get_active,
            commands::node::node_switch,
            commands::node::node_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
