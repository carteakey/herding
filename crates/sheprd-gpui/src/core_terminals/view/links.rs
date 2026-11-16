//! Links in a herdr-core pane. herdr-core has no link API, so the pane's own
//! frame is read as a Herdr pane's rows are by the row-local detector: web
//! addresses, `mailto:` and other explicit hyperlinks, and file paths. As
//! over a Herdr pane, nothing is read or drawn until the link modifier is
//! held; then the text under the pointer is verified (a path once a check off
//! the UI thread finds it on disk) and underlined, and only a click with the
//! modifier held opens it, through the window's own opener. A path is only
//! a link in a pane of this computer's core: another host's paths are not on
//! this disk, so there, as in a remote Herdr pane, they are plain text.

use super::CoreTerminalView;
use crate::{
    core_terminals::selection::Cell,
    terminal::{RowLink, RowTarget, frame_link_at},
    window::FileLink,
};
use gpui::{prelude::*, *};
use sheprd_backend::TerminalId;

/// A link a click opened in a herdr-core pane, for the window to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkClick {
    pub(crate) target: RowTarget,
    /// The pane's terminal and its working directory, which a relative path
    /// is read against.
    pub(crate) terminal: TerminalId,
    pub(crate) cwd: Option<String>,
    /// Whether Option was held, which swaps where the link opens.
    pub(crate) alt: bool,
}

#[derive(Default)]
pub(super) struct Links {
    /// The verified link under the pointer while the link modifier is held.
    hovered: Option<RowLink>,
    /// The path last looked for on disk, and whether it is there; `None`
    /// while the check runs.
    checked: Option<(FileLink, Option<bool>)>,
    /// Where the pointer last was, for a check's answer to be applied to.
    pointer: Option<(Point<Pixels>, Modifiers)>,
    /// The link a press with the modifier went down on.
    pressed: Option<RowLink>,
}

impl CoreTerminalView {
    /// The link at `cell`: any kind in a pane of this computer's core, and
    /// no file path in another host's.
    fn link_at(&self, cell: Cell) -> Option<RowLink> {
        frame_link_at(
            self.screen.frame.as_deref()?,
            cell.column,
            cell.row,
            &self.screen.wraps,
        )
        .filter(|link| !self.remote || !matches!(link.target, RowTarget::Path { .. }))
    }

    fn file_link(&self, target: &RowTarget) -> Option<FileLink> {
        let RowTarget::Path {
            path,
            line,
            explicit,
        } = target
        else {
            return None;
        };
        Some(FileLink {
            path: path.clone(),
            line: *line,
            pane_id: self.id.to_string(),
            cwd: self.cwd.clone(),
            explicit: *explicit,
        })
    }

    /// The rows of the verified link under the pointer, for the painter to
    /// underline.
    pub(crate) fn link_rows(&self) -> Vec<(u16, std::ops::Range<u16>)> {
        self.links
            .hovered
            .as_ref()
            .map(|link| link.rows().collect())
            .unwrap_or_default()
    }

    /// Follows the pointer and the link modifier, as over a Herdr pane. A
    /// pointer off the pane, over another one, hovers nothing here: the
    /// cell lookup would hold it on this pane's edge.
    pub(super) fn hover_link(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let inside = self.bounds.is_some_and(|bounds| bounds.contains(&position));
        self.links.pointer = inside.then_some((position, modifiers));
        let mut examined = false;
        let link = (inside && crate::keymap::secondary(&modifiers))
            .then(|| self.cell_at(position))
            .flatten()
            .and_then(|cell| self.link_at(cell))
            .and_then(|link| match &link.target {
                RowTarget::Web(_) | RowTarget::External(_) => Some(link),
                RowTarget::OtherHost(_) | RowTarget::Malformed => None,
                RowTarget::Path { .. } => {
                    let file = self.file_link(&link.target)?;
                    examined = true;
                    self.file_found(file, cx).then_some(link)
                }
            });
        // A path is looked for anew each time the user looks at it again.
        if !examined {
            self.links.checked = None;
        }
        if link != self.links.hovered {
            self.links.hovered = link;
            cx.notify();
        }
    }

    /// Follows the pointer at `position` with `modifiers` held, as a move or
    /// a modifier change over the window would.
    #[cfg(test)]
    pub(crate) fn hover_at(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        self.hover_link(position, modifiers, cx);
    }

    /// Forgets the hover once the pointer left the pane.
    pub(super) fn leave_link(&mut self, cx: &mut Context<Self>) {
        self.links.pointer = None;
        self.links.checked = None;
        if self.links.hovered.take().is_some() {
            cx.notify();
        }
    }

    /// Reads the hover again after the screen changed under the pointer. A
    /// path found missing is looked for again, as the output that redrew it
    /// may have made the file; one check runs at a time.
    pub(super) fn rehover_link(&mut self, cx: &mut Context<Self>) {
        if let Some((position, modifiers)) = self.links.pointer
            && crate::keymap::secondary(&modifiers)
        {
            if matches!(self.links.checked, Some((_, Some(false)))) {
                self.links.checked = None;
            }
            self.hover_link(position, modifiers, cx);
        }
    }

    /// Whether the file `link` names is on disk, as far as is known yet; the
    /// first question about it starts a check on the background executor.
    fn file_found(&mut self, link: FileLink, cx: &mut Context<Self>) -> bool {
        if let Some((checked, found)) = &self.links.checked
            && *checked == link
        {
            return *found == Some(true);
        }
        self.links.checked = Some((link.clone(), None));
        let asked = link.clone();
        let found = cx.background_executor().spawn(async move { asked.found() });
        cx.spawn(async move |this, cx| {
            let found = found.await;
            let _ = this.update(cx, |this, cx| {
                if !matches!(&this.links.checked, Some((checked, None)) if *checked == link) {
                    return;
                }
                this.links.checked = Some((link, Some(found)));
                if let Some((position, modifiers)) = this.links.pointer {
                    this.hover_link(position, modifiers, cx);
                }
            });
        })
        .detach();
        false
    }

    /// Takes a press with the link modifier on a link: an address, an
    /// explicit hyperlink, or a printed path no check has found missing. A
    /// press anywhere else goes on to select or to the program.
    pub(super) fn press_link(&mut self, event: &MouseDownEvent) -> bool {
        self.links.pressed = None;
        if event.button != MouseButton::Left
            || event.click_count != 1
            || !crate::keymap::secondary(&event.modifiers)
        {
            return false;
        }
        let Some(link) = self
            .cell_at(event.position)
            .and_then(|cell| self.link_at(cell))
        else {
            return false;
        };
        // A program reading the mouse keeps every press but one on a link
        // already verified under the pointer.
        if self.screen.mouse_reporting && self.links.hovered.as_ref() != Some(&link) {
            return false;
        }
        if let Some(file) = self.file_link(&link.target) {
            let missing =
                matches!(&self.links.checked, Some((checked, Some(false))) if *checked == file);
            if !file.explicit && missing {
                return false;
            }
        }
        self.links.pressed = Some(link);
        true
    }

    /// Opens the link a press went down on when the release is on it too;
    /// true when the press was a link's.
    pub(super) fn release_link(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(pressed) = self.links.pressed.take() else {
            return false;
        };
        // `cell_at` holds a point outside the cells on their edge, which a
        // release off the pane must not count as one on a link there.
        let inside = self
            .bounds
            .is_some_and(|bounds| bounds.contains(&event.position));
        let released = inside
            .then(|| self.cell_at(event.position))
            .flatten()
            .and_then(|cell| self.link_at(cell));
        if released.as_ref() != Some(&pressed) || !crate::keymap::secondary(&event.modifiers) {
            return true;
        }
        let click = LinkClick {
            target: pressed.target,
            terminal: self.id.clone(),
            cwd: self.cwd.clone(),
            alt: event.modifiers.alt,
        };
        if let Some(to_window) = &self.to_window {
            to_window(super::ToWindow::OpenLink(click), window, cx);
        }
        true
    }
}
