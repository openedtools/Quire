//! A message window for start-up problems, because a release build has no console.

#[cfg(any(windows, target_os = "macos"))]
pub fn show_error(title: &str, body: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(body)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn show_error(title: &str, body: &str) {
    // log-ok: dialog text written by the app on Linux; no meeting data
    log::error!("{title}: {body}");
}
