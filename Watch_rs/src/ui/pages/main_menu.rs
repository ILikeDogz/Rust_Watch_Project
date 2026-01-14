// Render the main menu page based on the current menu state.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

use crate::ui::state::MainMenuState;
use crate::ui::{
    assets::{get_cached_asset, precache_asset, AssetId},
    draw_image_bytes, PanelRgb565,
};

/// Render the main menu page based on the current menu state.
pub fn render(disp: &mut impl PanelRgb565, menu_state: MainMenuState) {
    match menu_state {
        MainMenuState::Home => {
            // Draw the cached Omnitrix logo asset (no FB mirror)
            if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                draw_image_bytes(disp, buf, w, h, false, false);
            } else if precache_asset(AssetId::Logo) {
                // Try again after precaching
                if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                    draw_image_bytes(disp, buf, w, h, false, false);
                }
            }
        }
        MainMenuState::WatchApp => {
            // Draw the cached watch icon asset (no FB mirror)
            let _ = disp.clear(Rgb565::BLACK);
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::WatchIcon) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else if precache_asset(AssetId::WatchIcon) {
                // Try again after precaching
                if let Some((bytes, w, h)) = get_cached_asset(AssetId::WatchIcon) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }
        MainMenuState::SettingsApp => {
            // Draw the cached settings image asset (no FB mirror)
            let _ = disp.clear(Rgb565::BLACK);
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::SettingsImage) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else if precache_asset(AssetId::SettingsImage) {
                // Try again after precaching
                if let Some((bytes, w, h)) = get_cached_asset(AssetId::SettingsImage) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }
        MainMenuState::GamesApp => {
            // Draw the cached games icon asset (no FB mirror)
            let _ = disp.clear(Rgb565::BLACK);
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::GamesIcon) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else if precache_asset(AssetId::GamesIcon) {
                // Try again after precaching
                if let Some((bytes, w, h)) = get_cached_asset(AssetId::GamesIcon) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }
    }
}
