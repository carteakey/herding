//! Herding's brand tokens (docs/herding-brand.md): the accent and status
//! palette in light and dark, resolved against the active theme so surfaces
//! can stay on the user's chosen theme while brand moments stay on brand.
//!
//! These are product colors, not the terminal palette: they decorate the
//! team surface (headers, status chips, empty states) and never replace the
//! theme's own background/foreground contract.

use crate::config::Theme;

/// Light-mode accent: the lead chevron, primary actions, focus.
pub(crate) const ACCENT_LIGHT: u32 = 0xC25E1E;
/// Dark-mode accent; slightly desaturated so it does not vibrate on near-black.
pub(crate) const ACCENT_DARK: u32 = 0xE8823C;

/// Light-mode companion to [`ACCENT_LIGHT`]: trailing chevrons, secondary
/// highlights.
pub(crate) const ACCENT_MUTED_LIGHT: u32 = 0xE8C4A8;
/// Dark-mode companion to [`ACCENT_DARK`].
pub(crate) const ACCENT_MUTED_DARK: u32 = 0x8A4E22;

/// A task in progress; the pulse color.
pub(crate) const STATUS_RUNNING_LIGHT: u32 = 0x2F6FEB;
/// A task in progress on a dark surface.
pub(crate) const STATUS_RUNNING_DARK: u32 = 0x5B8DEF;

/// Assigned/unclaimed work, held wakes, queued prompts.
pub(crate) const STATUS_WAITING_LIGHT: u32 = 0xB2730A;
/// The waiting color on a dark surface.
pub(crate) const STATUS_WAITING_DARK: u32 = 0xD9A03F;

/// Completed work, acknowledged bulletins, green gates.
pub(crate) const STATUS_DONE_LIGHT: u32 = 0x1F7A43;
/// The done color on a dark surface.
pub(crate) const STATUS_DONE_DARK: u32 = 0x4CBB7F;

/// Blocked tasks, failed deliveries, permission prompts.
pub(crate) const STATUS_BLOCKED_LIGHT: u32 = 0xC03434;
/// The blocked color on a dark surface.
pub(crate) const STATUS_BLOCKED_DARK: u32 = 0xE56868;

/// True when the theme's background is dark, which is how every token below
/// picks its variant. Midpoint is relative luminance 0.5 on the background.
pub(crate) fn is_dark(theme: &Theme) -> bool {
    let channel = |shift: u32| ((theme.background >> shift) & 255) as f32 / 255.;
    // The sRGB relative-luminance weights, squared like the spec's curve.
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    luminance < 0.5
}

/// The brand palette resolved for `theme`'s light or dark background.
pub(crate) struct Brand;

impl Brand {
    pub(crate) fn accent(theme: &Theme) -> u32 {
        if is_dark(theme) {
            ACCENT_DARK
        } else {
            ACCENT_LIGHT
        }
    }

    pub(crate) fn accent_muted(theme: &Theme) -> u32 {
        if is_dark(theme) {
            ACCENT_MUTED_DARK
        } else {
            ACCENT_MUTED_LIGHT
        }
    }

    pub(crate) fn running(theme: &Theme) -> u32 {
        if is_dark(theme) {
            STATUS_RUNNING_DARK
        } else {
            STATUS_RUNNING_LIGHT
        }
    }

    pub(crate) fn waiting(theme: &Theme) -> u32 {
        if is_dark(theme) {
            STATUS_WAITING_DARK
        } else {
            STATUS_WAITING_LIGHT
        }
    }

    pub(crate) fn done(theme: &Theme) -> u32 {
        if is_dark(theme) {
            STATUS_DONE_DARK
        } else {
            STATUS_DONE_LIGHT
        }
    }

    pub(crate) fn blocked(theme: &Theme) -> u32 {
        if is_dark(theme) {
            STATUS_BLOCKED_DARK
        } else {
            STATUS_BLOCKED_LIGHT
        }
    }
}

#[cfg(test)]
mod tests;
