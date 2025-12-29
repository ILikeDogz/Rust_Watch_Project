// Basic drawing utilities for the UI.

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoFont, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{Line, PrimitiveStyle},
    text::{Alignment, Text},
    Drawable,
};

use crate::ui::{PanelRgb565, RESOLUTION};

// helper function to draw centered text
pub fn draw_text(
    disp: &mut impl PanelRgb565,
    text: &str,
    fg: Rgb565,
    bg: Option<Rgb565>,
    x_point: i32,
    y_point: i32,
    clear: bool,
    update_fb: bool,
    font: Option<&'static MonoFont<'static>>,
) {
    if clear {
        // Prefer no-FB clear if available and requested
        if !update_fb {
            if let Some(co) = (disp as &mut dyn core::any::Any)
                .downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let _ = co.fill_rect_solid_no_fb(
                    0,
                    0,
                    RESOLUTION as u16,
                    RESOLUTION as u16,
                    Rgb565::BLACK,
                );
            } else {
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }

    // Use provided font or default
    let font = font.unwrap_or(&FONT_10X20);
    let mut builder = MonoTextStyleBuilder::new().font(font).text_color(fg);
    if let Some(b) = bg {
        builder = builder.background_color(b);
    }
    let style = builder.build();
    Text::with_alignment(text, Point::new(x_point, y_point), style, Alignment::Center)
        .draw(disp)
        .ok();
}

// Convert 24-bit RGB to Rgb565
pub fn rgb565_from_888(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new((r >> 3) as u8, (g >> 2) as u8, (b >> 3) as u8)
}

// Calculate the end point of a hand given center, angle, and length
pub fn hand_end(cx: i32, cy: i32, angle_deg: f32, length: i32) -> Point {
    let ang = angle_deg.to_radians();
    let dx = (libm::cosf(ang) * length as f32) as i32;
    let dy = (libm::sinf(ang) * length as f32) as i32;
    Point::new(cx + dx, cy + dy)
}

// Draw a hand line from center to end point
pub fn draw_hand_line(
    disp: &mut impl PanelRgb565,
    cx: i32,
    cy: i32,
    end: Point,
    color: Rgb565,
    stroke: u8,
) {
    let style = PrimitiveStyle::with_stroke(color, stroke.into());
    let _ = Line::new(Point::new(cx, cy), end)
        .into_styled(style)
        .draw(disp);
}
