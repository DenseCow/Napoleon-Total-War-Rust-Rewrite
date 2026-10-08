//! Helpers for tests that run UI scripts (this crate's unit tests and its integration tests).

use super::UiScriptHost;

/// Asserts the host logged no script error since the last drain (and drains the log, printing it
/// for `--nocapture`).
pub fn no_errors(host: &UiScriptHost) {
    let log = host.take_log();
    for l in &log {
        println!("  {l}");
    }
    let errors: Vec<_> = log.iter().filter(|l| l.starts_with("ERROR")).collect();
    assert!(errors.is_empty(), "script errors: {errors:#?}");
}
