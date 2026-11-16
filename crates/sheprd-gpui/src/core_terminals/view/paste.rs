//! Pasting into a herdr-core pane: the clipboard's text goes to the
//! program as a paste, which the terminal brackets when the program asks.
//! The same keys paste as over a Herdr pane, and the Edit menu's Paste
//! lands here rather than in the Herdr pane behind this one. Files copied
//! in a file manager, or dropped on the pane, paste their paths with shell
//! characters escaped, as over a local Herdr pane. Only this computer's core
//! sees the same files: another host's pane refuses them and says so, as
//! nothing carries them there yet. Enter is never sent.

use super::{CoreTerminalView, ToWindow};
use gpui::{App, ExternalPaths, Keystroke, Window};
use sheprd_backend::ClientPaneInputEvent;
use std::path::PathBuf;

/// Whether `keystroke` pastes: Cmd-V, Ctrl-Shift-V, or Shift-Insert, and
/// plain Ctrl-V where it pastes (Windows, and Linux, whose desktops send it
/// for a universal paste) while the clipboard holds text.
pub(crate) fn pastes(
    keystroke: &Keystroke,
    ctrl_v_pastes: bool,
    clipboard_has_text: impl FnOnce() -> bool,
) -> bool {
    let modifiers = keystroke.modifiers;
    let v = keystroke.key.eq_ignore_ascii_case("v");
    let insert = keystroke.key == "insert";
    if modifiers.alt || modifiers.function {
        return false;
    }
    if (modifiers.platform && !modifiers.control && v)
        || (modifiers.control && modifiers.shift && (v || insert))
        || (modifiers.shift && !modifiers.control && !modifiers.platform && insert)
    {
        return true;
    }
    let plain_ctrl_v = modifiers.control && !modifiers.shift && !modifiers.platform && v;
    ctrl_v_pastes && plain_ctrl_v && clipboard_has_text()
}

impl CoreTerminalView {
    /// Sends the clipboard's copied files' paths, or else its text, as a
    /// paste; false when it holds neither or this view only watches.
    pub(super) fn paste_clipboard(&self, window: &mut Window, cx: &mut App) -> bool {
        if self.observe {
            return false;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        if let Some(paths) = crate::window::external_paths(&item) {
            return self.paste_paths(&paths, window, cx);
        }
        let Some(text) = item.text().filter(|text| !text.is_empty()) else {
            return false;
        };
        self.send(ClientPaneInputEvent::Paste(text));
        true
    }

    /// Pastes the paths of files dropped on the pane, which takes the
    /// keyboard as the window does for a dropped-on Herdr pane.
    pub(crate) fn drop_files(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut App) {
        if self.observe || paths.paths().is_empty() {
            return;
        }
        cx.stop_propagation();
        if self.paste_paths(paths.paths(), window, cx) {
            window.focus(&self.focus, cx);
        }
    }

    /// Pastes `paths` quoted for a shell; a drop the quoting refuses (too
    /// many paths, control characters) is said, never pasted in part.
    fn paste_paths(&self, paths: &[PathBuf], window: &mut Window, cx: &mut App) -> bool {
        let quoted = if self.remote {
            Err(crate::window::file_drop::Error::OtherHost.into())
        } else {
            crate::window::file_drop::quote_paths(paths)
        };
        match quoted {
            Ok(text) => {
                self.send(ClientPaneInputEvent::Paste(text));
                true
            }
            Err(error) => {
                if let Some(to_window) = &self.to_window {
                    to_window(ToWindow::Refused(error), window, cx);
                }
                false
            }
        }
    }
}
