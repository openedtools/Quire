use log::info;
use tauri::{AppHandle, Emitter, Manager};

use super::manager::DatabaseManager;
use crate::state::AppState;

/// Initialize database on app startup
/// Handles first launch detection and conditional initialization
pub async fn initialize_database_on_startup(app: &AppHandle) -> Result<(), String> {
    // Check if this is the first launch (no database exists yet)
    let is_first_launch = DatabaseManager::is_first_launch(app)
        .await
        .map_err(|e| format!("Failed to check first launch status: {}", e))?;

    if is_first_launch {
        info!("First launch detected - will notify window when ready");

        // Delay event emission to ensure window is ready and React listeners are registered
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            app_handle
                .emit("first-launch-detected", ())
                .expect("Failed to emit first-launch-detected event");
            info!("Emitted first-launch-detected after delay");
        });
    } else {
        // Normal flow - initialize database immediately
        let db_manager = DatabaseManager::new_from_app_handle(app)
            .await
            .map_err(|e| format!("Failed to initialize database manager: {}", e))?;

        app.manage(AppState { db_manager });
        info!("Database initialized successfully");
        // Retention sweep at launch and every hour; it also finishes pending deletes (spec F-RET-4)
        crate::retention::commands::start_runner(app.clone());
    // Plain-text API keys move to the credential store (spec F-MOD-6)
    if let Some(state) = app.try_state::<AppState>() {
        crate::secrets::migrate_plaintext_keys(state.db_manager.pool()).await;
        // After the keys, which the upgrade rule reads (spec F-MOD-5)
        crate::cloud::initialize(state.db_manager.pool()).await;
        // Catch up the vault with anything that changed while the app was closed
        crate::vault::schedule(state.db_manager.pool(), crate::vault::DELAY);
    }
    }

    Ok(())
}
