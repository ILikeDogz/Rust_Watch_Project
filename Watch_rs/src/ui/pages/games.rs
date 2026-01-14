// Render the Games page.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

use crate::ui::draw::draw_text;
use crate::ui::{GamesState, PanelRgb565, CENTER};

/// Render the Games page based on the current state.
pub fn render(disp: &mut impl PanelRgb565, _state: GamesState) {
    let _ = disp.clear(Rgb565::BLACK);
    draw_text(
        disp,
        "game wip",
        Rgb565::WHITE,
        Some(Rgb565::BLACK),
        CENTER,
        CENTER,
        false,
        true,
        None,
    );
}
