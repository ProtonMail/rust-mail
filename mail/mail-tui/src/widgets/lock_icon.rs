use proton_crypto_inbox::lock_icon::{LockColor, LockIcon, UiLock};
use ratatui::style::{Color, Style, Stylize};

use crate::CLI_ARGS;

pub fn lock_icon_to_text(lock: UiLock) -> (&'static str, Style) {
    let app_config = &CLI_ARGS;
    let use_emoji = app_config.use_emoji;
    let lock_str = match (lock.icon, use_emoji) {
        (LockIcon::None, _) => "??",
        (LockIcon::ClosedLock, true) => "🔒",
        (LockIcon::ClosedLock, false) => "CL",
        (LockIcon::ClosedLockWithTick, true) => "🔒✔",
        (LockIcon::ClosedLockWithTick, false) => "CT",
        (LockIcon::ClosedLockWithPen, true) => "🔒✏",
        (LockIcon::ClosedLockWithPen, false) => "CP",
        (LockIcon::ClosedLockWarning, true) => "🔒⚠",
        (LockIcon::ClosedLockWarning, false) => "CW",
        (LockIcon::OpenLockWithPen, true) => "🔓✏",
        (LockIcon::OpenLockWithPen, false) => "OP",
        (LockIcon::OpenLockWithTick, true) => "🔓✔",
        (LockIcon::OpenLockWithTick, false) => "OT",
        (LockIcon::OpenLockWarning, true) => "🔓⚠",
        (LockIcon::OpenLockWarning, false) => "OW",
    };

    let lock_style = match lock.color {
        LockColor::Black => Style::default(),
        LockColor::Green => Style::default().bg(Color::Green).fg(Color::White),
        LockColor::Blue => Style::default().bg(Color::Blue).fg(Color::White),
    }
    .bold();

    (lock_str, lock_style)
}
