//! EXT: where log lines go. The engine used to `println!` -- fine for a demo run from a
//! terminal, useless for a game launched from Finder, where stdout is nowhere and the only
//! record of a bad frame or a refused vsync is the player's memory of it.
//!
//! Two sinks through the `log` facade: the terminal, at the level the command line or
//! `DAYDREAMS_LOG` asks for (default info), and a file in the per-user log directory, which
//! is what the crash dialog points at. The newest five files are kept.
//!
//! `simplelog` rather than `fern`: both are mature, but `fern` is a formatting toolkit -- the
//! two sinks, their levels and the "time + level + message" line would each be hand-built --
//! where `simplelog` ships exactly this shape as `CombinedLogger(TermLogger, WriteLogger)`
//! with a `Config` per sink, and its `TermLogger` already does the colour and tty detection.
//! Less code here means less to get wrong in the one module that reports everything else.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use log::LevelFilter;
use simplelog::{ColorChoice, CombinedLogger, ConfigBuilder, SharedLogger, TermLogger, TerminalMode, WriteLogger};

/// Files older than the newest this many are deleted at startup.
const KEEP: usize = 5;

/// Log targets dropped from both sinks. `symphonia`, the decoder behind kira, narrates every
/// track it opens ("using xing header for duration", "skipped N bytes of junk") at info and
/// warn -- facts about the file, not the game, repeated on every title load.
const DECODER_TARGET: &str = "symphonia";

/// The file the current session logs to, for the crash dialog. `None` before `init`, or when
/// the file sink could not be opened or was switched off.
static FILE: OnceLock<PathBuf> = OnceLock::new();

pub fn file_path() -> Option<&'static Path> {
    FILE.get().map(PathBuf::as_path)
}

/// The level to run at: `--log-level` wins, then `DAYDREAMS_LOG`, then info. An unparsable
/// environment value is reported (once the logger is up) rather than silently ignored.
pub fn level_from(flag: Option<LevelFilter>) -> (LevelFilter, Option<String>) {
    if let Some(level) = flag {
        return (level, None);
    }
    match std::env::var("DAYDREAMS_LOG") {
        Ok(raw) => match raw.parse::<LevelFilter>() {
            Ok(level) => (level, None),
            Err(_) => (LevelFilter::Info, Some(raw)),
        },
        Err(_) => (LevelFilter::Info, None),
    }
}

/// Install both sinks. Nothing here can stop the game: a log directory that cannot be created
/// leaves the terminal sink alone and says so.
pub fn init(flag: Option<LevelFilter>, file_sink: bool) {
    let (level, bad_env) = level_from(flag);

    // Terminal: the message and its level, nothing else -- `[INFO] [shot] wrote ...` is what
    // the project's own tooling greps for, and a timestamp in front would only be noise on a
    // developer's terminal.
    let term_config = ConfigBuilder::new()
        .set_time_level(LevelFilter::Off)
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Off)
        .add_filter_ignore_str(DECODER_TARGET)
        .build();
    let mut sinks: Vec<Box<dyn SharedLogger>> =
        vec![TermLogger::new(level, term_config, TerminalMode::Mixed, ColorChoice::Auto)];

    let mut file_error = None;
    if file_sink {
        match open_log_file() {
            Ok((path, file)) => {
                // Timestamps and module paths in the file: it is read after the fact, when
                // "which subsystem said this, and when" is the whole question. simplelog's
                // per-field levels mean "print this field for records at least this severe",
                // so `Error` in the target slot prints it on every line.
                let mut file_config = ConfigBuilder::new();
                file_config
                    .set_time_format_rfc3339()
                    .set_thread_level(LevelFilter::Off)
                    .set_target_level(LevelFilter::Error)
                    .set_location_level(LevelFilter::Off)
                    .add_filter_ignore_str(DECODER_TARGET);
                // The local offset is only available when no other thread exists yet (the
                // `time` crate refuses otherwise); UTC is the documented fallback.
                let _ = file_config.set_time_offset_to_local();
                // A plain `File`: every record is one unbuffered write, so the last lines
                // before a crash are on disk when the dialog opens rather than in a buffer
                // the process takes down with it.
                sinks.push(WriteLogger::new(level, file_config.build(), file));
                let _ = FILE.set(path);
            }
            Err(e) => file_error = Some(e),
        }
    }

    // A second `init` (there is none; tests never call this) would be the only way this fails.
    let _ = CombinedLogger::init(sinks);

    if let Some(raw) = bad_env {
        log::warn!("DAYDREAMS_LOG={raw:?} is not a log level (off, error, warn, info, debug, trace); using info");
    }
    if let Some(e) = file_error {
        log::warn!("no log file this session: {e}");
    }
}

/// The per-user log directory: `~/Library/Application Support/DayDreams/logs` on macOS,
/// `%LOCALAPPDATA%\DayDreams\data\logs` on Windows, `$XDG_DATA_HOME/daydreams/logs` on Linux.
fn log_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "DayDreams").map(|d| d.data_local_dir().join("logs"))
}

/// Create this session's file, then prune the directory down to the newest `KEEP` files.
fn open_log_file() -> std::io::Result<(PathBuf, File)> {
    let dir = log_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory to put a log directory in")
    })?;
    std::fs::create_dir_all(&dir)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(file_name(secs, std::process::id()));
    let file = File::create(&path)?;
    prune(&dir, KEEP);
    Ok((path, file))
}

/// `daydreams-YYYYMMDD-HHMMSS-PID.log`, UTC. The date sorts by name, which is what `prune`
/// relies on; the pid keeps two sessions started in the same second apart.
fn file_name(unix_secs: u64, pid: u32) -> String {
    let (y, m, d) = civil_from_days((unix_secs / 86_400) as i64);
    let s = unix_secs % 86_400;
    format!("daydreams-{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{pid}.log", s / 3600, (s / 60) % 60, s % 60)
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day). Howard Hinnant's
/// `civil_from_days`, which is exact for any date the `u64` above can name; it is here so the
/// file name needs no date crate.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Delete every `daydreams-*.log` in `dir` but the `keep` that sort last by name. Failures are
/// ignored: a log file that cannot be deleted is not worth refusing to start over.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("daydreams-") && n.ends_with(".log"))
        })
        .collect();
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    for old in &logs[..excess] {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        // 2000-02-29, a leap day in a century year that is a leap year.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_688), (2026, 8, 23));
    }

    #[test]
    fn file_names_sort_by_time() {
        let a = file_name(1_000_000_000, 1);
        let b = file_name(1_000_000_001, 1);
        assert_eq!(a, "daydreams-20010909-014640-1.log");
        assert!(a < b);
    }

    #[test]
    fn prune_keeps_the_newest_by_name() {
        let dir = std::env::temp_dir().join(format!("daydreams-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..7 {
            std::fs::write(dir.join(format!("daydreams-2026010{i}-000000-1.log")), "").unwrap();
        }
        std::fs::write(dir.join("unrelated.txt"), "").unwrap();
        prune(&dir, 5);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "daydreams-20260102-000000-1.log",
                "daydreams-20260103-000000-1.log",
                "daydreams-20260104-000000-1.log",
                "daydreams-20260105-000000-1.log",
                "daydreams-20260106-000000-1.log",
                "unrelated.txt",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn level_precedence() {
        // The flag wins over anything in the environment, whatever it holds.
        assert_eq!(level_from(Some(LevelFilter::Trace)).0, LevelFilter::Trace);
    }
}
