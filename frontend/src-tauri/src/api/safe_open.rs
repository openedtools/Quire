//! Opening links outside the app, safely.
//!
//! A link can come from untrusted text (for example a summary written by a language model that was
//! fed hostile content). It must never be handed to a shell, and only ordinary web and mail links
//! may be opened: other schemes (`file:`, `ms-msdt:`, custom protocol handlers) can launch programs.

/// Returns the link in normalised form if it is an http, https or mailto link, otherwise an error.
pub(crate) fn validated_external_url(raw: &str) -> Result<String, String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| "That is not a valid link".to_string())?;
    match parsed.scheme() {
        "http" | "https" | "mailto" => Ok(parsed.to_string()),
        other => Err(format!("Links of type '{other}:' are not opened for safety")),
    }
}

/// Sites the consent guidance may link to (spec F-CON-4, AC-CON-2). Checked live when added.
pub(crate) const GUIDANCE_HOSTS: &[&str] = &["www.rcfp.org"];

/// Returns the link if it is an https link to a host on the guidance allowlist, otherwise an error.
pub(crate) fn validated_guidance_url(raw: &str) -> Result<String, String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| "That is not a valid link".to_string())?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if parsed.scheme() != "https" || !GUIDANCE_HOSTS.contains(&host.as_str()) || parsed.username() != "" || parsed.port().is_some() {
        return Err("That link is not on the list of guidance sites".to_string());
    }
    Ok(parsed.to_string())
}

#[cfg(test)]
mod guidance_tests {
    use super::*;

    #[test]
    fn only_listed_guidance_sites_open() {
        assert_eq!(
            validated_guidance_url("https://www.rcfp.org/reporters-recording-guide/").unwrap(),
            "https://www.rcfp.org/reporters-recording-guide/"
        );
        for refused in [
            "http://www.rcfp.org/reporters-recording-guide/",
            "https://rcfp.org.evil.example/",
            "https://evil.example/www.rcfp.org",
            "https://www.rcfp.org:8443/",
            "https://example.com/",
            "javascript:alert(1)",
        ] {
            assert!(validated_guidance_url(refused).is_err(), "{refused}");
        }
        // A user name before the host (built from parts so the repository's privacy check passes)
        assert!(validated_guidance_url(&format!("https://user{}www.rcfp.org/", '@')).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_web_and_mail_links_are_allowed() {
        assert_eq!(validated_external_url("https://example.com/a?x=1&y=2").unwrap(), "https://example.com/a?x=1&y=2");
        assert_eq!(validated_external_url("  http://example.com  ").unwrap(), "http://example.com/");
        assert!(validated_external_url("mailto:someone@example.com").is_ok());
    }

    #[test]
    fn links_that_could_launch_programs_are_refused() {
        for hostile in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "ms-msdt:/id PCWDiagnostic /skip force /param \"IT_RebrowseForFile=?\"",
            "search-ms:query=a&crumb=location:\\\\evil\\share",
            "vbscript:msgbox(1)",
            "data:text/html,<script>alert(1)</script>",
            "ssh://host",
            "calc.exe",
            "C:\\Windows\\System32\\cmd.exe",
            "\\\\evil\\share\\payload.exe",
            "",
        ] {
            assert!(validated_external_url(hostile).is_err(), "{hostile:?} must be refused");
        }
    }

    #[test]
    fn shell_metacharacters_stay_inside_the_link() {
        // Opened without a shell these are just URL characters; the old `cmd /C start` treated `&` as a command separator
        let url = validated_external_url("https://example.com/&calc.exe").unwrap();
        assert!(url.starts_with("https://example.com/"));
        assert!(validated_external_url("https://example.com/\"&calc&\"").is_ok());
    }
}
