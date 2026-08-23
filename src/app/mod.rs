//! EXT: the application platform layer -- everything a shipped binary needs around the engine
//! that the C++ demo, a Visual Studio project launched from its own folder, never did.
//!
//! | Module   | Owns                                                                    |
//! |----------|-------------------------------------------------------------------------|
//! | `cli`    | The command line (clap): the dev flags and the `gen-terrain` subcommand |
//! | `logging` | The `log` facade's sinks: terminal plus a rotated per-user log file     |
//! | `crash`  | The panic hook, the native crash dialog and the one fatal-error sink     |
//! | `error`  | `AssetError`, the typed failure every asset loader returns              |
//! | `assets` | Where `Shaders/`, `Meshes/`, `Textures/` and `assets/` are found        |
//!
//! None of this is part of the port. `src/main.rs` calls into it in a fixed order -- panic hook,
//! command line, logging, asset root -- before the window exists, so that a failure in any of
//! them has somewhere to be reported.

pub mod assets;
pub mod cli;
pub mod crash;
pub mod error;
pub mod logging;
