//! Plain-text diagnostics at ~/Library/Logs/Reel/reel.log (truncated past 512 KB).

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

pub fn path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let dir = crate::test_mode::directory().cloned().unwrap_or_else(|| crate::home().join("Library/Logs/Reel"));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("reel.log")
    })
}

pub fn write(message: &str) {
    let p = path();
    if std::fs::metadata(p).map(|m| m.len() > 512_000).unwrap_or(false) {
        let _ = std::fs::write(p, b"");
    }
    let now = time_of_day();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{now} {message}");
    }
}

/// Local "HH:MM:SS.mmm".
fn time_of_day() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!("{:02}:{:02}:{:02}.{:03}", tm.tm_hour, tm.tm_min, tm.tm_sec, now.subsec_millis())
}
