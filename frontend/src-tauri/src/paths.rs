//! Locations inside the app's data folder for code that has no Tauri app handle.

use crate::brand_generated as brand;
use std::path::PathBuf;

/// The app's data folder: the platform data directory plus the bundle identifier. It is the same
/// folder Tauri's `app_data_dir()` returns, so everything stays in one place.
pub fn app_data_dir() -> Option<PathBuf> {
    dirs::data_dir().or_else(dirs::home_dir).map(|dir| dir.join(brand::BUNDLE_ID))
}

/// Where the user's own summary templates live.
pub fn custom_templates_dir() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("templates"))
}

/// Where notification settings are stored.
pub fn notification_settings_file() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("notifications.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_lives_under_the_identifier_folder() {
        let base = app_data_dir().expect("a data folder");
        assert!(base.ends_with(brand::BUNDLE_ID));
        assert_eq!(custom_templates_dir().unwrap(), base.join("templates"));
        assert_eq!(notification_settings_file().unwrap(), base.join("notifications.json"));
    }
}
