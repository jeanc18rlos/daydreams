# DayDreams task runner (https://github.com/casey/just). `just` lists the targets.
#
# Everything here is a plain cargo invocation. `just` runs from the directory of this file,
# and the game itself does not care where it is started from: src/app/assets.rs resolves an
# asset root once at startup (see README, "Asset root").

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list --unsorted

# Run the game fullscreen, release profile.
run *ARGS:
    cargo run --release -- {{ARGS}}

# Run the game in a 1280x720 window. The form to use while developing: a fullscreen
# crash leaves the display wherever it was.
run-windowed *ARGS:
    cargo run --release -- --windowed {{ARGS}}

# The whole suite, release profile: the collision and mesh tests are too slow unoptimised,
# and several parse the shipped meshes, so this needs Meshes/ checked out.
test *ARGS:
    cargo test --release {{ARGS}}

# What CI runs: formatting, clippy over every target with warnings as errors.
lint:
    cargo fmt --check
    cargo clippy --release --all-targets -- -D warnings

# Licence, advisory and duplicate-version audit of the dependency tree (needs cargo-deny).
deny:
    cargo deny check

# Screenshot SCENE into OUT (a .bmp) after the physics has settled, no window needed
# beyond the one the GL context lives in. `just shot 14 out.bmp`.
shot SCENE OUT *ARGS:
    cargo run --release -- --windowed --scene {{SCENE}} --shot {{OUT}} --frames 120 {{ARGS}}

# Frame-cost measurement: 600 frames of SCENE with vsync off, numbers on stdout as
# `[shot] ... avg frame X ms, p95 Y ms`. Fullscreen, to match the README's table; the
# screenshot it has to write goes to a gitignored file.
bench SCENE="14":
    cargo run --release -- --scene {{SCENE}} --shot out_bench.bmp --frames 600 --no-vsync

# Regenerate Meshes/meadow_tile.obj from ext::terrain::height. Run after changing the
# height function; a test fails if the mesh and the function disagree.
gen-terrain:
    cargo run --release -- gen-terrain

# The shipping binary: release plus stripped symbols. Lands in target/dist/daydreams.
dist:
    cargo build --profile dist

# DayDreams.app (macOS) with the asset directories inside Contents/Resources. Needs
# `cargo install cargo-bundle`. Output under target/dist/bundle/.
bundle: dist
    cargo bundle --profile dist
