//! The retention setting and the pure decision of when a meeting's audio is due (spec F-RET-1,
//! F-RET-2, F-RET-4, F-RET-13). No database or files here; the clock is passed in.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// How long audio is kept after a recording is finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Retention {
    /// Deleted as soon as the transcript is safely saved.
    Off,
    H24,
    D7,
    D14,
    D30,
    Keep,
}

impl Retention {
    pub const ALL: [Retention; 6] = [Retention::Off, Retention::H24, Retention::D7, Retention::D14, Retention::D30, Retention::Keep];

    /// The stored form, also used by the vault format (`sync-format.md` section 10).
    pub fn as_str(self) -> &'static str {
        match self {
            Retention::Off => "off",
            Retention::H24 => "h24",
            Retention::D7 => "d7",
            Retention::D14 => "d14",
            Retention::D30 => "d30",
            Retention::Keep => "keep",
        }
    }

    pub fn parse(text: &str) -> Option<Retention> {
        Retention::ALL.into_iter().find(|r| r.as_str() == text.trim())
    }

    /// How long audio lives after finalization; None for Keep.
    pub fn period(self) -> Option<Duration> {
        match self {
            Retention::Off => Some(Duration::zero()),
            Retention::H24 => Some(Duration::hours(24)),
            Retention::D7 => Some(Duration::days(7)),
            Retention::D14 => Some(Duration::days(14)),
            Retention::D30 => Some(Duration::days(30)),
            Retention::Keep => None,
        }
    }

    /// The deletion deadline for audio finalized at `finalized_at`.
    pub fn deadline(self, finalized_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.period().map(|p| finalized_at + p)
    }
}

/// What the sweep should do with one meeting's audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Not finalized yet: never touched (F-RET-2).
    NotFinalized,
    /// No audio left to delete.
    NoAudio,
    /// Kept by the setting or by "Keep this one".
    Keep,
    /// Not due yet.
    NotYet,
    /// Due now.
    Delete,
}

/// The retention fields of one meeting.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPolicy {
    pub retention: Option<Retention>,
    pub expires_at: Option<DateTime<Utc>>,
    pub finalized_at: Option<DateTime<Utc>>,
    /// `present`, `none` or `deleted`.
    pub state: String,
}

/// Whether one meeting's audio is due at `now`. Due means `now` is at or after the deadline.
pub fn due(policy: &AudioPolicy, now: DateTime<Utc>) -> Decision {
    if policy.finalized_at.is_none() {
        return Decision::NotFinalized;
    }
    if policy.state != "present" {
        return Decision::NoAudio;
    }
    match (policy.retention, policy.expires_at) {
        (None, _) | (Some(Retention::Keep), _) | (_, None) => Decision::Keep,
        (Some(_), Some(deadline)) if now >= deadline => Decision::Delete,
        _ => Decision::NotYet,
    }
}

/// More than 20 recordings at once, or more than a quarter of all recordings with audio, needs
/// the person's confirmation (F-RET-13). The quarter rule applies from 4 recordings due, so a
/// small library is not asked about every pair of recordings.
pub fn needs_confirmation(due_count: usize, with_audio: usize) -> bool {
    due_count > 20 || (due_count >= 4 && due_count * 4 > with_audio)
}

/// A clock earlier than one already seen (by more than five minutes of slack) pauses deletion.
pub fn clock_went_back(now: DateTime<Utc>, latest_seen: Option<DateTime<Utc>>) -> bool {
    latest_seen.map_or(false, |seen| now + Duration::minutes(5) < seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
    }

    fn policy(retention: Retention, finalized: DateTime<Utc>) -> AudioPolicy {
        AudioPolicy {
            retention: Some(retention),
            expires_at: retention.deadline(finalized),
            finalized_at: Some(finalized),
            state: "present".to_string(),
        }
    }

    #[test]
    fn each_setting_is_due_exactly_at_its_deadline_and_not_a_millisecond_before() {
        let ended = t(2, 14);
        for (r, period) in [
            (Retention::Off, Duration::zero()),
            (Retention::H24, Duration::hours(24)),
            (Retention::D7, Duration::days(7)),
            (Retention::D14, Duration::days(14)),
            (Retention::D30, Duration::days(30)),
        ] {
            let p = policy(r, ended);
            let deadline = ended + period;
            assert_eq!(due(&p, deadline), Decision::Delete, "{r:?} at the deadline");
            assert_eq!(due(&p, deadline - Duration::milliseconds(1)), Decision::NotYet, "{r:?} 1 ms before");
        }
        let keep = policy(Retention::Keep, ended);
        assert_eq!(keep.expires_at, None);
        assert_eq!(due(&keep, t(2, 14) + Duration::days(10_000)), Decision::Keep);
    }

    #[test]
    fn unfinalized_and_audio_less_meetings_are_never_due() {
        let mut p = policy(Retention::Off, t(2, 14));
        p.finalized_at = None;
        assert_eq!(due(&p, t(30, 0)), Decision::NotFinalized);
        let mut p = policy(Retention::Off, t(2, 14));
        p.state = "deleted".to_string();
        assert_eq!(due(&p, t(30, 0)), Decision::NoAudio);
        p.state = "none".to_string();
        assert_eq!(due(&p, t(30, 0)), Decision::NoAudio);
    }

    #[test]
    fn stored_names_round_trip() {
        for r in Retention::ALL {
            assert_eq!(Retention::parse(r.as_str()), Some(r));
        }
        assert_eq!(Retention::parse("forever"), None);
    }

    #[test]
    fn mass_expiry_and_the_clock_guard() {
        assert!(needs_confirmation(21, 1000));
        assert!(!needs_confirmation(20, 1000));
        assert!(needs_confirmation(5, 16), "more than a quarter");
        assert!(!needs_confirmation(4, 16), "exactly a quarter");
        assert!(!needs_confirmation(3, 4), "small libraries are not asked about a few recordings");
        assert!(clock_went_back(t(2, 10), Some(t(2, 11))));
        assert!(!clock_went_back(t(2, 11), Some(t(2, 11))));
        assert!(!clock_went_back(t(2, 11), None));
    }
}
