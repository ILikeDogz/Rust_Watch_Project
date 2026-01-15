// Shared helpers for games rendering.

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, Size},
    primitives::Rectangle,
    Drawable,
};

use crate::ui::{draw::draw_text, PanelRgb565, RESOLUTION};

// Clear a box area on the framebuffer display and return the rectangle coordinates.
pub fn clear_box_fb(
    display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    half_w: i32,
    half_h: i32,
    bg: Rgb565,
) -> (i32, i32, i32, i32) {
    let x0 = (center_x - half_w).max(0);
    let y0 = (center_y - half_h).max(0);
    let x1 = (center_x + half_w).min((RESOLUTION - 1) as i32);
    let y1 = (center_y + half_h).min((RESOLUTION - 1) as i32);
    let _ = crate::display::FastPanelOps::fill_rect_fb(display, x0, y0, x1, y1, bg);
    (x0, y0, x1, y1)
}

// Clear a box area on the given display.
pub fn clear_box(
    disp: &mut impl PanelRgb565,
    center_x: i32,
    center_y: i32,
    half_w: i32,
    half_h: i32,
    bg: Rgb565,
) {
    let _ = Rectangle::new(
        Point::new(center_x - half_w, center_y - half_h),
        Size::new((half_w * 2) as u32, (half_h * 2) as u32),
    )
    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(bg))
    .draw(disp);
}

// Draw multiple lines of text centered at (center_x, center_y) on the framebuffer display.
pub fn draw_lines_fb(
    display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    lines: &[(&str, i32)],
    fg: Rgb565,
    bg: Rgb565,
) {
    for (text, yoff) in lines {
        draw_text(
            display,
            text,
            fg,
            Some(bg),
            center_x,
            center_y + *yoff,
            false,
            true,
            None,
        );
    }
}

// Draw multiple lines of text centered at (center_x, center_y) on the given display.
pub fn draw_lines(
    disp: &mut impl PanelRgb565,
    center_x: i32,
    center_y: i32,
    lines: &[(&str, i32)],
    fg: Rgb565,
    bg: Rgb565,
) {
    for (text, yoff) in lines {
        draw_text(
            disp,
            text,
            fg,
            Some(bg),
            center_x,
            center_y + *yoff,
            false,
            true,
            None,
        );
    }
}
