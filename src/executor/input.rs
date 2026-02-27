// Physical input simulation — full implementation in Phase 5.
use crate::errors::{SeeClawError, SeeClawResult};

pub async fn mouse_click(_x: i32, _y: i32) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Executor not implemented yet (Phase 5)".to_string()))
}

pub async fn mouse_double_click(_x: i32, _y: i32) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Executor not implemented yet (Phase 5)".to_string()))
}

pub async fn mouse_right_click(_x: i32, _y: i32) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Executor not implemented yet (Phase 5)".to_string()))
}

pub async fn type_text(_text: &str, _clear_first: bool) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Executor not implemented yet (Phase 5)".to_string()))
}

pub async fn press_hotkey(_keys: &str) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Executor not implemented yet (Phase 5)".to_string()))
}
