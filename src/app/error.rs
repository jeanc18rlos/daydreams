//! EXT: the one error type every asset loader returns.
//!
//! The ported loaders (`Shader::new`, `Texture::new`, `Mesh::new`) inherited the original's
//! attitude to failure -- a missing shader was an empty source string that failed to compile
//! into a `.log` file nobody read, a missing mesh drew nothing -- and the port turned each of
//! those into a `panic!` with a message. A panic is the right *outcome* for a packaging bug,
//! but it is the wrong *shape*: it cannot be tested without a GL context, it cannot be routed
//! to a dialog, and it carries no structure a log line could use. So the loaders return this,
//! and the single place that turns it into an exit is [`crate::app::crash::fatal`].

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("shader {path} failed to compile:\n{log}")]
    ShaderCompile { path: PathBuf, log: String },
    #[error("shader program '{name}' failed to link:\n{log}")]
    ShaderLink { name: String, log: String },
    #[error("{path} is not a usable BMP: {reason}")]
    BadBmp { path: PathBuf, reason: String },
    #[error("{path}: {reason}")]
    Gltf { path: PathBuf, reason: String },
    #[error("OpenGL: {0}")]
    Gl(String),
    /// No candidate directory held the game's `Shaders/`; `tried` is every place looked, in
    /// the order `app::assets` looks, so the dialog can say what was searched.
    #[error("no asset directory found; tried:{}", tried.iter().map(|p| format!("\n  {}", p.display())).collect::<String>())]
    NoAssetRoot { tried: Vec<PathBuf> },
}
