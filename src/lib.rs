//! Local, synthetic-only Snipboard prototype helper.
//!
//! The shell process never loads this code: it runs as a separate persistent
//! worker speaking JSON lines over stdin/stdout.

pub mod core;
pub mod dev;
pub mod json;
pub mod search;
pub mod storage;
pub mod sys;
pub mod worker;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Storage failures (`Io`) are kept distinct from rejected data (`Invalid`), because
/// callers treat an ambiguous persistence failure more cautiously than a validation error.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => write!(f, "{error}"),
            Error::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The repository holding this build. Prototype data must never live inside it,
/// because the shell watches the whole installation and reloads on writes.
pub fn source_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(root) = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize() {
        roots.push(root);
    }
    // The repository can be relocated after building (see `dev`), so also trust the
    // nearest ancestor of the running executable that looks like this plugin.
    if let Some(root) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
        .and_then(|exe| {
            exe.ancestors()
                .find(|dir| dir.join("manifest.json").is_file() && dir.join("Cargo.toml").is_file())
                .map(Path::to_path_buf)
        })
        && !roots.contains(&root)
    {
        roots.push(root);
    }
    roots
}

/// The most specific source root, used by tools that operate on the repository.
pub fn source_root() -> PathBuf {
    source_roots()
        .pop()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}
