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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, Weak};
use std::thread::ThreadId;

use winit::window::Window;

use super::error::AssetError;

/// The thread `install` ran on -- main -- which is the only one the dialog is shown from.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Set by [`set_headless`] for runs that have nobody at the screen (`--shot`, `gen-terrain`):
/// the dialog is skipped, the log line is all there is.
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// The game window, once it exists, so the hook can hand the display back before it blocks
/// on the dialog. `Weak`: the hook must not keep a window alive that the app is dropping, and
/// a dropped window (the panic came from the teardown) simply means there is nothing to undo.
static WINDOW: Mutex<Weak<Window>> = Mutex::new(Weak::new());

/// Headless means no dialog, whatever else is set. `main` decides from the command line:
/// `--shot` and `gen-terrain` are driven by scripts and CI, which cannot press OK -- and on
/// macOS the alert is drawn by a system daemon, not by this process, so a modal nobody
/// dismisses stays on the desktop after the process is killed. `DAYDREAMS_NO_DIALOG` in the
/// environment is the same switch for a run that is headless for some other reason.
pub fn set_headless(headless: bool) {
    HEADLESS.store(headless, Ordering::Relaxed);
}

/// Register the game window. Once, from `main`'s thread, as soon as the window exists.
pub fn register_window(window: Weak<Window>) {
    if let Ok(mut slot) = WINDOW.lock() {
        *slot = window;
    }
}

/// Give the display back before the dialog: ungrab and show the cursor, leave fullscreen.
/// Without this a panic in a fullscreen run put the alert behind a black borderless window
/// with the pointer locked -- the dialog was there, invisible, and so was the desktop. Main
/// thread only: winit's window methods on macOS must be called from it, and a second panic
/// inside the hook is an abort.
///
/// `try_lock`, not `lock`: a panic while `register_window` held the mutex would deadlock the
/// hook; skipping the release is the lesser harm.
fn release_display() {
    let Ok(slot) = WINDOW.try_lock() else { return };
    let Some(window) = slot.upgrade() else { return };
    let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
    window.set_cursor_visible(true);
    window.set_fullscreen(None);
}

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
    let dialog = dialog_allowed();
    if !dialog {
        log::info!("no crash dialog: headless run, or not the main thread");
    }
    log::logger().flush();
    if dialog {
        release_display();
        show_dialog("DayDreams crashed", &format!("{message}\n\nat {location}"));
    }
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
    if dialog_allowed() {
        show_dialog("DayDreams cannot start", &err.to_string());
    }
    std::process::exit(1);
}

/// Whether a dialog may be shown at all: from the main thread only, and not for a headless
/// run -- `--shot` and `gen-terrain` through [`set_headless`], or `DAYDREAMS_NO_DIALOG` in the
/// environment. A headless run has nobody to press OK, and a modal it cannot dismiss is a
/// hang where an exit code was wanted; on macOS it is worse than a hang, because the alert is
/// drawn by a system daemon on the process's behalf and stays on the desktop after the process
/// is killed. The log line carries the same text either way.
///
/// Off the main thread, rfd on macOS would block the caller while it dispatches the alert to
/// the main thread; if main is at that moment joining the thread that panicked (the glTF
/// decoder's scoped threads are joined exactly so), neither side can proceed. The scope's own
/// re-raise of the panic reaches this hook on main a moment later and shows the dialog then,
/// so nothing is lost by logging only here.
fn dialog_allowed() -> bool {
    MAIN_THREAD.get() == Some(&std::thread::current().id())
        && !HEADLESS.load(Ordering::Relaxed)
        && std::env::var_os("DAYDREAMS_NO_DIALOG").is_none()
}

/// A native error dialog with the log file's path appended. Only after [`dialog_allowed`].
fn show_dialog(title: &str, body: &str) {
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
