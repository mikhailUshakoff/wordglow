// Tauri + Leptos app shell — wired up in a later phase (Library / Reading
// & playback). For now this just proves the workspace and `core` link up.
fn main() -> core::Result<()> {
    core::db::open_default()?;
    println!("wordglow.db ready at {:?}", core::paths::db_path()?);
    Ok(())
}
