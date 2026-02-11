// Render the Games page.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

use crate::ui::draw::draw_text;
use crate::ui::{
    assets::{get_cached_asset, precache_asset, AssetId},
    draw_image_bytes,
    games::{play_pong, play_snake},
    GamesState, PanelRgb565, CENTER,
};

// Render the Games page based on the current state.
pub fn render(disp: &mut impl PanelRgb565, state: GamesState) {
    let _ = precache_asset(AssetId::PongIcon);
    let _ = precache_asset(AssetId::SnakeIcon);
    match state {
        GamesState::MenuPong => {
            let _ = disp.clear(Rgb565::BLACK);
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::PongIcon) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else {
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
        }
        GamesState::MenuSnake => {
            let _ = disp.clear(Rgb565::BLACK);
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::SnakeIcon) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else {
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
        }
        GamesState::Snake => play_snake(disp),
        GamesState::Pong => play_pong(disp),
    }
}
