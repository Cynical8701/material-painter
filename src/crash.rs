//! Crash reports: when the app panics, what happened (message, place,
//! backtrace, version, system and GPU) is written to `crash.log` in the
//! settings folder. The next start finds it, keeps it under a dated name
//! and offers to open it.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The settings folder (`%APPDATA%/MaterialPainter` and equivalents).
pub fn data_dir() -> Option<PathBuf> {
    // Tests never touch the user's settings, recovery files or nodes.
    if cfg!(test) {
        return Some(std::env::temp_dir().join("MaterialPainterTest"));
    }
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("MaterialPainter"))
}

fn log_path() -> Option<PathBuf> {
    Some(data_dir()?.join("crash.log"))
}

/// What the report says about the machine (the GPU, once known).
static CONTEXT: OnceLock<String> = OnceLock::new();

/// Records the GPU and driver for crash reports.
pub fn set_context(context: String) {
    let _ = CONTEXT.set(context);
}

/// Writes a crash report on any panic, then runs the default handler.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let place = info.location().map_or_else(|| "unknown".into(), |l| format!("{}:{}", l.file(), l.line()));
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let report = format!(
            "Material Painter {} crashed\n\nWhen: {when} (seconds since 1970, UTC)\nThread: {}\nSystem: {} {}\nGPU: {}\n\nMessage: {message}\nAt: {place}\n\nBacktrace:\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::thread::current().name().unwrap_or("unnamed"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            CONTEXT.get().map_or("unknown", |s| s.as_str()),
            std::backtrace::Backtrace::force_capture(),
        );
        if let Some(path) = log_path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(mut f) = std::fs::File::create(&path) {
                let _ = f.write_all(report.as_bytes());
            }
        }
        previous(info);
    }));
}

/// A report left by the last run: moved to a dated name (so it shows once)
/// and returned.
pub fn take_report() -> Option<PathBuf> {
    let path = log_path()?;
    if !path.is_file() {
        return None;
    }
    let when = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let secs = when.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let kept = path.with_file_name(format!("crash-{secs}.log"));
    std::fs::rename(&path, &kept).ok()?;
    Some(kept)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_report_is_taken_once() {
        let path = super::log_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "test crash").unwrap();
        let kept = super::take_report().expect("report found");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "test crash");
        assert!(super::take_report().is_none(), "shown once");
        let _ = std::fs::remove_file(kept);
    }
}
