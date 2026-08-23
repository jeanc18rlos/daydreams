//! EXT: where the game's files are.
//!
//! The port loads everything through relative paths -- `Shaders/`, `Meshes/`, `Textures/`,
//! `assets/` -- exactly as `Resources.cpp` does, which means the working directory had to be
//! the project root or the first shader failed to open. A binary launched from Finder, a
//! `.app` bundle, or `cargo run` from another directory all break that. The root is resolved
//! once, here, and every loader joins onto it.
//!
//! Resolution order, first hit wins, where a hit is a directory containing `Shaders/`:
//!
//! 1. `--assets DIR` or `DAYDREAMS_ASSETS` -- and an explicit choice that does not qualify is
//!    an error rather than a fall-through, because silently using some other directory is
//!    exactly what someone setting the variable was trying to prevent;
//! 2. `../Resources` relative to the executable when it lives in `*.app/Contents/MacOS/`;
//! 3. the executable's own directory;
//! 4. the current directory;
//! 5. the crate root baked in at build time (`CARGO_MANIFEST_DIR`), so `cargo run` works from
//!    anywhere on the machine that built it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::error::AssetError;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The resolved root. Resolved on first use if `init` was not called (tests), so every test
/// that reads an asset finds the crate's own.
pub fn root() -> &'static Path {
    ROOT.get_or_init(|| resolve_or_die(None))
}

/// Resolve once, from main, with the command line's choice. Fatal if nothing qualifies.
pub fn init(flag: Option<PathBuf>) -> &'static Path {
    ROOT.get_or_init(|| resolve_or_die(flag))
}

/// `rel` under the root: `path("Shaders/sky.frag")`.
pub fn path(rel: &str) -> PathBuf {
    root().join(rel)
}

fn resolve_or_die(flag: Option<PathBuf>) -> PathBuf {
    let explicit = flag.or_else(|| std::env::var_os("DAYDREAMS_ASSETS").map(PathBuf::from));
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    resolve(&candidates(explicit, exe.as_deref(), cwd.as_deref(), &manifest), has_shaders)
        .unwrap_or_else(|e| super::crash::fatal(&e))
}

/// What makes a directory an asset root: it holds `Shaders/`. The first file every run opens
/// is a shader, so a root without them fails on its first load anyway; the other three
/// directories are not checked because the intro needs none of them before the first frame.
fn has_shaders(dir: &Path) -> bool {
    dir.join("Shaders").is_dir()
}

/// The directories to try, in order. An explicit choice is the whole list.
fn candidates(
    explicit: Option<PathBuf>,
    exe: Option<&Path>,
    cwd: Option<&Path>,
    manifest: &Path,
) -> Vec<PathBuf> {
    if let Some(dir) = explicit {
        // Absolute, so the log line names a place rather than a name relative to a
        // working directory the reader may not know.
        return vec![std::path::absolute(&dir).unwrap_or(dir)];
    }
    let mut out = Vec::new();
    if let Some(exe_dir) = exe.and_then(Path::parent) {
        if let Some(resources) = bundle_resources(exe_dir) {
            out.push(resources);
        }
        out.push(exe_dir.to_path_buf());
    }
    if let Some(cwd) = cwd {
        out.push(cwd.to_path_buf());
    }
    out.push(manifest.to_path_buf());
    out
}

/// `Foo.app/Contents/MacOS` -> `Foo.app/Contents/Resources`, where a bundle keeps its files.
fn bundle_resources(exe_dir: &Path) -> Option<PathBuf> {
    let contents = exe_dir.parent()?;
    let app = contents.parent()?;
    let is_bundle = exe_dir.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app";
    is_bundle.then(|| contents.join("Resources"))
}

/// The first candidate `qualifies`, or every one tried.
fn resolve(
    candidates: &[PathBuf],
    qualifies: impl Fn(&Path) -> bool,
) -> Result<PathBuf, AssetError> {
    candidates
        .iter()
        .find(|dir| qualifies(dir))
        .cloned()
        .ok_or_else(|| AssetError::NoAssetRoot { tried: candidates.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn explicit_choice_is_the_only_candidate() {
        let c = candidates(
            Some(p("/x/assets")),
            Some(Path::new("/bin/daydreams")),
            Some(Path::new("/cwd")),
            Path::new("/src"),
        );
        assert_eq!(c, vec![p("/x/assets")]);
    }

    #[test]
    fn search_order_without_a_choice() {
        let c = candidates(
            None,
            Some(Path::new("/Applications/DayDreams.app/Contents/MacOS/daydreams")),
            Some(Path::new("/cwd")),
            Path::new("/src"),
        );
        assert_eq!(
            c,
            vec![
                p("/Applications/DayDreams.app/Contents/Resources"),
                p("/Applications/DayDreams.app/Contents/MacOS"),
                p("/cwd"),
                p("/src"),
            ]
        );
        // Not a bundle: no Resources entry.
        let c = candidates(
            None,
            Some(Path::new("/opt/daydreams/target/release/daydreams")),
            None,
            Path::new("/src"),
        );
        assert_eq!(c, vec![p("/opt/daydreams/target/release"), p("/src")]);
    }

    #[test]
    fn first_qualifying_candidate_wins() {
        let c = vec![p("/a"), p("/b"), p("/c")];
        let got = resolve(&c, |d| d == Path::new("/b") || d == Path::new("/c")).unwrap();
        assert_eq!(got, p("/b"));
    }

    #[test]
    fn nothing_qualifying_lists_everything_tried() {
        let c = vec![p("/a"), p("/b")];
        let err = resolve(&c, |_| false).unwrap_err();
        match &err {
            AssetError::NoAssetRoot { tried } => assert_eq!(tried, &c),
            other => panic!("unexpected {other:?}"),
        }
        let text = err.to_string();
        assert!(text.contains("/a") && text.contains("/b"), "{text}");
    }

    #[test]
    fn only_a_directory_with_shaders_qualifies() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("bare");
        let file = tmp.path().join("file");
        let full = tmp.path().join("full");
        std::fs::create_dir(&bare).unwrap();
        std::fs::create_dir(&full).unwrap();
        std::fs::create_dir(full.join("Shaders")).unwrap();
        // A `Shaders` that is a file, not a directory, does not count either.
        std::fs::create_dir(&file).unwrap();
        std::fs::write(file.join("Shaders"), b"").unwrap();
        assert!(!has_shaders(&bare) && !has_shaders(&file) && has_shaders(&full));
        assert!(!has_shaders(&tmp.path().join("missing")));
        // The search order is honoured over real directories: the first one that qualifies
        // wins even when a later one would too, and a missing one is skipped.
        let order =
            vec![tmp.path().join("missing"), bare.clone(), file.clone(), full.clone(), p("/")];
        assert_eq!(resolve(&order, has_shaders).unwrap(), full);
        let err = resolve(&[bare.clone(), file.clone()], has_shaders).unwrap_err();
        assert!(matches!(&err, AssetError::NoAssetRoot { tried } if tried.len() == 2));
        // An explicit choice is checked and not fallen through: a bare directory named on
        // the command line is the whole list, and it fails.
        let c = candidates(Some(bare.clone()), Some(&full.join("daydreams")), Some(&full), &full);
        assert_eq!(c, vec![bare.clone()]);
        assert!(resolve(&c, has_shaders).is_err());
        // Without a choice, the executable's directory is tried before the working directory
        // and the crate: an exe beside a `Shaders/` finds it.
        let c = candidates(None, Some(&full.join("daydreams")), Some(&bare), &bare);
        assert_eq!(resolve(&c, has_shaders).unwrap(), full);
        let c = candidates(None, Some(&bare.join("daydreams")), Some(&full), &bare);
        assert_eq!(resolve(&c, has_shaders).unwrap(), full);
    }

    #[test]
    fn the_crate_root_resolves_in_tests() {
        assert!(root().join("Shaders").is_dir());
        assert!(path("Meshes/quad.obj").is_file());
    }
}
