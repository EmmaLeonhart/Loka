//! Stamp the build with its source commit and time.
//!
//! Why this exists: `loka --version` printed only `CARGO_PKG_VERSION`, so a
//! binary built in May and one built today both said `loka 0.4.1`. That made two
//! months of drift invisible, and it cost real time three separate ways —
//! `loka serve` binding 0.0.0.0 (the overnight Windows Firewall prompts) after
//! the 127.0.0.1 fix was already in source; three "engine bugs" investigated
//! against a stale binary that did not reproduce on current source; and, on
//! 2026-07-30, a verification run failing on queries the parser had just been
//! fixed to accept, because the release binary predated the fix by an hour.
//!
//! A version string that cannot distinguish two builds is not a version string.
//! Now: `loka 0.4.1 (306a148 2026-07-30T05:12:00Z)`.
//!
//! Best-effort by design: if git is unavailable (a source tarball, a sandboxed
//! build) the sha becomes `unknown` and the build still succeeds. A build stamp
//! is diagnostic information, not a dependency.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let sha = git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(out) if !out.is_empty() => "-dirty",
        _ => "",
    };
    println!("cargo:rustc-env=LOKA_BUILD_SHA={}{}", sha, dirty);
    println!("cargo:rustc-env=LOKA_BUILD_TIME={}", utc_now());

    // Re-run when HEAD moves, so the stamp cannot go stale on an incremental
    // build — the exact failure this is meant to make visible.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Minimal RFC-3339 UTC timestamp. Days-since-epoch → civil date via the
/// standard algorithm, so this needs no date crate.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Howard Hinnant's days-from-civil inverse (public domain).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
