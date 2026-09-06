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

    dotenvy::dotenv().ok();
    let ollama_config: commands::OllamaConfig =
        envy::from_env().expect("reading OLLAMA_URL / OLLAMA_MODEL from the environment");

    tauri::Builder::default()
        .manage(commands::DbState(std::sync::Mutex::new(conn)))
        .manage(ollama_config)
        .invoke_handler(tauri::generate_handler![
            commands::list_books,
            commands::list_lessons,
            commands::get_lesson,
            commands::get_lesson_audio,
            commands::get_lesson_questions,
            commands::get_lesson_statuses,
            commands::set_lesson_status,
            commands::save_word,
            commands::remove_word,
            commands::list_dictionary,
            commands::translate_word,
            commands::import_package
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    leptos::mount::mount_to_body(app::App);
}
