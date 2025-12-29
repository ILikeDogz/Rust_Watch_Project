// Render the Easter Egg (Info) page.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

use crate::ui::assets::{draw_image_bytes, INFO_PAGE_IMAGE};
use crate::ui::draw::draw_text;
use crate::ui::{PanelRgb565, CENTER};

/// Render the Easter Egg (Info) page.
pub fn render(disp: &mut impl PanelRgb565) {
    // Draw info page image by decompressing on demand (no cache).
    let need = (466 * 466 * 2) as usize;
    if let Ok(buf) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(INFO_PAGE_IMAGE, need)
    {
        // If decompression worked and got the expected size, draw the image.
        if buf.len() == need {
            draw_image_bytes(disp, &buf, 466, 466, false, false);
        } else {
            // Fallback: clear screen and draw text.
            disp.clear(Rgb565::WHITE).ok();
            draw_text(
                disp,
                "Info Screen",
                Rgb565::CYAN,
                None,
                CENTER,
                CENTER,
                false,
                true,
                None,
            );
        }
    } else {
        // Fallback: clear screen and draw text.
        disp.clear(Rgb565::WHITE).ok();
        draw_text(
            disp,
            "Info Screen",
            Rgb565::CYAN,
            None,
            CENTER,
            CENTER,
            false,
            true,
            None,
        );
    }
}
