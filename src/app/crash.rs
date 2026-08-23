//! EXT: what happens when the game cannot go on.
//!
//! The game starts fullscreen with the cursor locked, so before this a panic was a screen that
//! went black and a desktop that came back, with the message on a stdout nobody launched from
//! Finder could see. Two paths end here:
//!
//! * a **panic**, through the hook `install` puts in place: logged with its location and a
//!   backtrace, shown in a native dialog with the log file's path, and then handed to the
//!   default hook so the process still dies the way Rust's runtime expects (`panic =
//!   "unwind"`, which winit relies on to unwind out of its callbacks);
//! * an **asset failure**, through [`fatal`]: a loader returned an [`AssetError`] and the
//!   caller is a `Resources::acquire_*` whose hundred call sites have no `Result` to carry
//!   it. Logged, shown, and the process exits with code 1.

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::sync::OnceLock;
use std::thread::ThreadId;

use super::error::AssetError;

/// The thread `install` ran on -- main -- which is the only one the dialog is shown from.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Install the panic hook. Before anything else in `main`, so that even a failure in
/// argument parsing or logging setup is reported through it.
pub fn install() {
    let _ = MAIN_THREAD.set(std::thread::current().id());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report_panic(info);
        default_hook(info);
    }));
}

fn report_panic(info: &PanicHookInfo) {
    let message = panic_message(info);
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| String::from("unknown location"));
    // force_capture: the hook must not depend on RUST_BACKTRACE being set in the environment
    // of a game launched by double-click, where nothing sets it.
    let backtrace = Backtrace::force_capture();
    log::error!("panic: {message}\n  at {location}\n{backtrace}");
    log::logger().flush();
    show_dialog("DayDreams crashed", &format!("{message}\n\nat {location}"));
}

/// The payload a `panic!` carries is a `&str` or a `String`; anything else is shown as such.
fn panic_message(info: &PanicHookInfo) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("(non-string panic payload)")
    }
}

/// An asset could not be loaded and the caller cannot carry the error. Never returns.
pub fn fatal(err: &AssetError) -> ! {
    log::error!("fatal: {err}");
    log::logger().flush();
    show_dialog("DayDreams cannot start", &err.to_string());
    std::process::exit(1);
}

/// A native error dialog with the log file's path appended -- from the main thread only, and
/// not when `DAYDREAMS_NO_DIALOG` is set: a headless run (a CI screenshot job, a script
/// driving `--shot`) has nobody to press OK, and a modal it cannot dismiss is a hang where an
/// exit code was wanted. The log line carries the same text either way.
///
/// Off the main thread, rfd on macOS would block the caller while it dispatches the alert to
/// the main thread; if main is at that moment joining the thread that panicked (the glTF
/// decoder's scoped threads are joined exactly so), neither side can proceed. The scope's own
/// re-raise of the panic reaches this hook on main a moment later and shows the dialog then,
/// so nothing is lost by logging only here.
fn show_dialog(title: &str, body: &str) {
    if MAIN_THREAD.get() != Some(&std::thread::current().id()) || std::env::var_os("DAYDREAMS_NO_DIALOG").is_some() {
        return;
    }
    let mut text = body.to_string();
    match super::logging::file_path() {
        Some(path) => text.push_str(&format!("\n\nDetails were written to\n{}", path.display())),
        None => text.push_str("\n\n(no log file this session)"),
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
