// Render the Omnitrix page based on the current Omnitrix state.

use crate::ui::assets::{asset_id_for_state, draw_image_bytes, get_cached_asset, precache_asset};
use crate::ui::state::OmnitrixState;
use crate::ui::PanelRgb565;

// Render the Omnitrix page based on the current Omnitrix state.
pub fn render(disp: &mut impl PanelRgb565, state: OmnitrixState) {
    let aid = asset_id_for_state(state);
    if let Some((bytes, w, h)) = get_cached_asset(aid) {
        draw_image_bytes(disp, bytes, w, h, false, false);
    } else if precache_asset(aid) {
        if let Some((bytes, w, h)) = get_cached_asset(aid) {
            draw_image_bytes(disp, bytes, w, h, false, false);
        }
    }
}
