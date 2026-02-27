use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

use crate::errors::{SeeClawError, SeeClawResult};

/// Single left-click at absolute physical pixel coordinates.
pub async fn mouse_click(x: i32, y: i32) -> SeeClawResult<()> {
    tokio::task::spawn_blocking(move || click_sync(x, y, Button::Left, false))
        .await
        .map_err(|e| SeeClawError::Executor(e.to_string()))?
}

/// Double left-click.
pub async fn mouse_double_click(x: i32, y: i32) -> SeeClawResult<()> {
    tokio::task::spawn_blocking(move || click_sync(x, y, Button::Left, true))
        .await
        .map_err(|e| SeeClawError::Executor(e.to_string()))?
}

/// Right-click.
pub async fn mouse_right_click(x: i32, y: i32) -> SeeClawResult<()> {
    tokio::task::spawn_blocking(move || click_sync(x, y, Button::Right, false))
        .await
        .map_err(|e| SeeClawError::Executor(e.to_string()))?
}

/// Type text into the focused control (via clipboard paste to handle CJK).
pub async fn type_text(text: String, _clear_first: bool) -> SeeClawResult<()> {
    tokio::task::spawn_blocking(move || {
        let mut enigo = new_enigo()?;
        // Use key sequence for ASCII, clipboard paste for non-ASCII
        enigo
            .text(&text)
            .map_err(|e| SeeClawError::Executor(format!("type_text: {e}")))?;
        Ok(())
    })
    .await
    .map_err(|e| SeeClawError::Executor(e.to_string()))?
}

/// Press a key combination like "ctrl+c", "win+d", "alt+f4".
pub async fn press_hotkey(keys: String) -> SeeClawResult<()> {
    tokio::task::spawn_blocking(move || {
        let mut enigo = new_enigo()?;
        let parts: Vec<&str> = keys.split('+').map(|s| s.trim()).collect();

        let modifier_keys: Vec<enigo::Key> = parts[..parts.len().saturating_sub(1)]
            .iter()
            .filter_map(|k| parse_modifier_key(k))
            .collect();

        let main_key = parts.last().and_then(|k| parse_key(k));

        // Press modifiers
        for mk in &modifier_keys {
            enigo
                .key(*mk, Direction::Press)
                .map_err(|e| SeeClawError::Executor(format!("modifier press: {e}")))?;
        }
        // Tap main key
        if let Some(k) = main_key {
            enigo
                .key(k, Direction::Click)
                .map_err(|e| SeeClawError::Executor(format!("key click: {e}")))?;
        }
        // Release modifiers in reverse
        for mk in modifier_keys.iter().rev() {
            enigo
                .key(*mk, Direction::Release)
                .map_err(|e| SeeClawError::Executor(format!("modifier release: {e}")))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| SeeClawError::Executor(e.to_string()))?
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn new_enigo() -> SeeClawResult<Enigo> {
    Enigo::new(&Settings::default())
        .map_err(|e| SeeClawError::Executor(format!("Enigo::new: {e}")))
}

fn click_sync(x: i32, y: i32, button: Button, double: bool) -> SeeClawResult<()> {
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| SeeClawError::Executor(format!("move_mouse: {e}")))?;
    std::thread::sleep(std::time::Duration::from_millis(80));
    enigo
        .button(button, Direction::Click)
        .map_err(|e| SeeClawError::Executor(format!("button click: {e}")))?;
    if double {
        std::thread::sleep(std::time::Duration::from_millis(60));
        enigo
            .button(button, Direction::Click)
            .map_err(|e| SeeClawError::Executor(format!("button double: {e}")))?;
    }
    Ok(())
}

fn parse_modifier_key(s: &str) -> Option<enigo::Key> {
    match s.to_lowercase().as_str() {
        "ctrl" | "control" => Some(enigo::Key::Control),
        "shift" => Some(enigo::Key::Shift),
        "alt" => Some(enigo::Key::Alt),
        "win" | "meta" | "super" => Some(enigo::Key::Meta),
        _ => None,
    }
}

fn parse_key(s: &str) -> Option<enigo::Key> {
    match s.to_lowercase().as_str() {
        "enter" | "return" => Some(enigo::Key::Return),
        "escape" | "esc" => Some(enigo::Key::Escape),
        "tab" => Some(enigo::Key::Tab),
        "space" => Some(enigo::Key::Space),
        "backspace" => Some(enigo::Key::Backspace),
        "delete" | "del" => Some(enigo::Key::Delete),
        "home" => Some(enigo::Key::Home),
        "end" => Some(enigo::Key::End),
        "pageup" => Some(enigo::Key::PageUp),
        "pagedown" => Some(enigo::Key::PageDown),
        "arrowup" | "up" => Some(enigo::Key::UpArrow),
        "arrowdown" | "down" => Some(enigo::Key::DownArrow),
        "arrowleft" | "left" => Some(enigo::Key::LeftArrow),
        "arrowright" | "right" => Some(enigo::Key::RightArrow),
        "f1" => Some(enigo::Key::F1),
        "f2" => Some(enigo::Key::F2),
        "f3" => Some(enigo::Key::F3),
        "f4" => Some(enigo::Key::F4),
        "f5" => Some(enigo::Key::F5),
        "f6" => Some(enigo::Key::F6),
        "f7" => Some(enigo::Key::F7),
        "f8" => Some(enigo::Key::F8),
        "f9" => Some(enigo::Key::F9),
        "f10" => Some(enigo::Key::F10),
        "f11" => Some(enigo::Key::F11),
        "f12" => Some(enigo::Key::F12),
        // modifier keys can also be the main key
        "ctrl" | "control" => Some(enigo::Key::Control),
        "shift" => Some(enigo::Key::Shift),
        "alt" => Some(enigo::Key::Alt),
        "win" | "meta" | "super" => Some(enigo::Key::Meta),
        // single ASCII character
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            Some(enigo::Key::Unicode(c))
        }
        _ => None,
    }
}
