//! Retry for file operations that Windows antivirus scanners, search indexers and sync clients
//! briefly block ("Access is denied" or a sharing violation right after a file was written).

use std::fs;
use std::io;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

/// Run `op` up to `tries` times, waiting `delay` between attempts. A missing file is not retried.
pub fn retry<T>(tries: u32, delay: Duration, mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut attempt = 1;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(e) if attempt >= tries || e.kind() == io::ErrorKind::NotFound => return Err(e),
            Err(e) => {
                log::info!("File operation failed (attempt {attempt} of {tries}): {e}; retrying");
                sleep(delay);
                attempt += 1;
            }
        }
    }
}

/// Rename, retrying a few times.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    retry(5, Duration::from_millis(300), || fs::rename(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn succeeds_after_transient_failures() {
        let calls = Cell::new(0);
        let result = retry(5, Duration::from_millis(1), || {
            calls.set(calls.get() + 1);
            if calls.get() < 3 {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "held by a scanner"))
            } else {
                Ok("done")
            }
        });
        assert_eq!(result.unwrap(), "done");
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn gives_up_after_the_given_number_of_tries() {
        let calls = Cell::new(0);
        let result: io::Result<()> = retry(4, Duration::from_millis(1), || {
            calls.set(calls.get() + 1);
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "still held"))
        });
        assert!(result.is_err());
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn a_missing_file_is_not_retried() {
        let calls = Cell::new(0);
        let result: io::Result<()> = retry(5, Duration::from_millis(1), || {
            calls.set(calls.get() + 1);
            Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
        });
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn rename_moves_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"x").unwrap();
        rename(&a, &b).unwrap();
        assert!(!a.exists() && b.exists());
    }
}
