// Render the Games page.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

use crate::ui::draw::draw_text;
use crate::ui::{games::play_pong, GamesState, PanelRgb565, CENTER};

// Render the Games page based on the current state.
pub fn render(disp: &mut impl PanelRgb565, state: GamesState) {
    match state {
        GamesState::MenuPong => {
            let _ = disp.clear(Rgb565::BLACK);
            draw_text(
                disp,
                "Pong",
                Rgb565::WHITE,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                false,
                true,
                None,
            );
        }
        GamesState::MenuSnake => {
            let _ = disp.clear(Rgb565::BLACK);
            draw_text(
                disp,
                "Snake",
                Rgb565::WHITE,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                false,
                true,
                None,
            );
        }
        GamesState::Snake => {
            let _ = disp.clear(Rgb565::BLACK);
            draw_text(
                disp,
                "WIP",
                Rgb565::WHITE,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                false,
                true,
                None,
            );
        }
        GamesState::Pong => play_pong(disp),
    }
}
