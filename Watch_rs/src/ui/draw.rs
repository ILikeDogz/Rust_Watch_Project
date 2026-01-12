// Basic drawing utilities for the UI.

use embedded_graphics::{
    image::{Image, ImageRawBE},
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
        // Prefer no-FB clear if available and requested, slightly faster on some backends
        if !update_fb {
            if let Some(co) = (disp as &mut dyn core::any::Any)
                .downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
                    co,
                    0,
                    0,
                    RESOLUTION as u16,
                    RESOLUTION as u16,
                    Rgb565::BLACK,
                );
            } else {
                // fallback clear
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            // normal clear (uses embedded-graphics implementation which may or may not use FB depending on backend)
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

// Draw raw RGB565 bytes centered on the display.
pub fn draw_image_bytes(
    disp: &mut impl PanelRgb565,
    bytes: &[u8],
    w: u32,
    h: u32,
    clear: bool,
    update_fb: bool,
) {
    // Clear background if requested
    if clear {
        if !update_fb {
            if let Some(co) = (disp as &mut dyn core::any::Any)
                .downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
                    co,
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
    // Validate size
    if bytes.len() != (w * h * 2) as usize {
        return;
    }
    let x = (RESOLUTION.saturating_sub(w)) as i32 / 2;
    let y = (RESOLUTION.saturating_sub(h)) as i32 / 2;

    // Try fast raw blit when the backend supports it.
    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let res = if update_fb {
            crate::display::FastPanelOps::blit_rect_be_fast(
                co,
                x as u16,
                y as u16,
                w as u16,
                h as u16,
                bytes,
            )
        } else {
            crate::display::FastPanelOps::blit_rect_be_fast_no_fb(
                co,
                x as u16,
                y as u16,
                w as u16,
                h as u16,
                bytes,
            )
        };
        if res.is_ok() {
            return;
        }
        if let Err(e) = res {
            esp_println::println!("fast blit failed: {:?}; fallback", e);
        }
    }

    let raw = ImageRawBE::<Rgb565>::new(bytes, w);
    let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
}
