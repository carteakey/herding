//! Finds the GUI that `sheprd-launcher.exe`, the windowed Windows launcher, starts.

mod error;

use std::path::{Path, PathBuf};

pub use error::{Error, Result};

/// The GUI and CLI executable, installed beside the launcher.
const GUI_EXE: &str = "herding.exe";

/// The `herding.exe` in the launcher's own directory, or an older
/// `sheprd.exe` or `herdr-gpui.exe` installed before the rename.
///
/// # Errors
///
/// [`Error::NoParent`] when `launcher` has no parent directory.
pub fn gui_path(launcher: &Path) -> Result<PathBuf> {
    let directory = launcher.parent().ok_or_else(|| Error::NoParent {
        launcher: launcher.to_owned(),
    })?;
    let current = directory.join(GUI_EXE);
    if current.is_file() {
        return Ok(current);
    }
    for legacy in ["sheprd.exe", "herdr-gpui.exe"] {
        let path = directory.join(legacy);
        if path.is_file() {
            return Ok(path);
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests;
