mod dto;

#[cfg(not(target_arch = "wasm32"))]
mod commands;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod bindings;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let conn = wordglow_core::db::open_default().expect("failed to open wordglow.db");

    tauri::Builder::default()
        .manage(commands::DbState(std::sync::Mutex::new(conn)))
        .invoke_handler(tauri::generate_handler![commands::list_books, commands::list_lessons])
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    leptos::mount::mount_to_body(app::App);
}
