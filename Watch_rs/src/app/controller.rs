use crate::app::state;
use crate::ui;
use crate::ui::{Page, UiState, WatchAppState};

// Handle the "back" action from user input
pub fn handle_back() -> bool {
    if ui::watch_edit_active() {
        ui::watch_edit_cancel();
    } else {
        state::ui_state_update(UiState::back);
    }
    true
}

// Handle the "select" action from user input
pub fn handle_select() -> bool {
    let ui_state = state::ui_state_get();
    if matches!(ui_state.page, Page::Watch(WatchAppState::Digital)) {
        if ui::watch_edit_active() {
            ui::watch_edit_advance();
        } else {
            ui::watch_edit_start();
        }
    } else {
        state::ui_state_update(UiState::select);
    }
    true
}

// Handle the "transform" action from user input
pub fn handle_transform(in_omnitrix: bool) -> bool {
    state::ui_state_update(UiState::transform);
    in_omnitrix
}
