// Render the settings page based on the current settings menu state.F

extern crate alloc;

use alloc::format;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{Line, PrimitiveStyle, Rectangle},
    Drawable,
};
use libm::{cosf, sinf};

use crate::ui::{
    CENTER, PanelRgb565, RESOLUTION, 
    brightness::{
    brightness_edit_set, get_brightness_last_pct, get_brightness_pct, reset_brightness_last,
    set_brightness_last_pct}, 
    draw::{
        clear_ring_arc_fast, draw_ring_segment, draw_text, rgb565_from_888,
    },
    state::SettingsMenuState,
};

// Render the settings page based on the current settings menu state.
pub fn render(disp: &mut impl PanelRgb565, settings_state: SettingsMenuState) {

    // Common center coordinates
    let center_x = CENTER;
    let center_y = CENTER;

    match settings_state {
        SettingsMenuState::BrightnessPrompt => {

            // This reset_brightness_last call ensures that when entering brightness adjust mode, it forcees a full redraw.
            reset_brightness_last();
            brightness_edit_set(false);

            // Clear the screen, then draw a simple white sun icon with label inside, 
            // frame buffer update is also necessary for the additional graphics to appear correctly.
            let _ = disp.clear(Rgb565::BLACK);
            let outer_r = 90;
            let ray_len = 42;
            let ray_thick = 6u8;
            let sun_color = Rgb565::WHITE; // color for sun icon
            let stroke_width = 4u32;
            
            // Circle (embedded-graphics Circle uses top-left corner positioning)
            let _ = embedded_graphics::primitives::Circle::new(
                Point::new(center_x - outer_r, center_y - outer_r),
                (outer_r * 2) as u32,
            )
            .into_styled(PrimitiveStyle::with_stroke(sun_color, stroke_width))
            .draw(disp);
            
            // Rays: start just outside the circle's outer stroke edge
            // Circle outer edge is at outer_r, stroke extends inward, so outer visible edge is at outer_r
            // Add a small gap (2px) for clean separation
            let ray_start = outer_r as f32 + 6.0;
            let ray_end = outer_r as f32 + ray_len as f32;
            
            // Helper for rounding (no_std doesn't have f32::round)
            let round_i32 = |v: f32| -> i32 {
                if v >= 0.0 { (v + 0.5) as i32 } else { (v - 0.5) as i32 }
            };
            
            for i in 0..8 {
                // Draw rays at 45-degree intervals
                let ang = i as f32 * core::f32::consts::FRAC_PI_4;
                let cos = cosf(ang);
                let sin = sinf(ang);
                // Start point (just outside circle)
                let tx = center_x + round_i32(cos * ray_start);
                let ty = center_y + round_i32(sin * ray_start);
                // End point
                let rx = center_x + round_i32(cos * ray_end);
                let ry = center_y + round_i32(sin * ray_end);

                // Draw the ray line
                let _ = Line::new(Point::new(tx, ty), Point::new(rx, ry))
                    .into_styled(PrimitiveStyle::with_stroke(sun_color, ray_thick as u32))
                    .draw(disp);
            }

            // two layers of text to fit the sun icon
            draw_text(
                disp,
                "Adjust",
                sun_color,
                Some(Rgb565::BLACK),
                center_x,
                center_y - 8,
                false,
                false,
                None,
            );
            // second layer for better readability
            draw_text(
                disp,
                "Brightness",
                sun_color,
                Some(Rgb565::BLACK),
                center_x,
                center_y + 8,
                false,
                false,
                None,
            );
        }
        SettingsMenuState::BrightnessAdjust => {
            draw_brightness_ui(disp);
        }
        SettingsMenuState::EasterEgg => {
            draw_text(
                disp,
                "Easter Egg",
                Rgb565::WHITE,
                Some(Rgb565::BLACK),
                center_x,
                center_y,
                true,
                true,
                None,
            );
        }
    }
}


// Draw the brightness adjustment UI
fn draw_brightness_ui(my_display: &mut impl PanelRgb565) {
    // Draw a ring representing brightness percentage and numeric value in center
    let pct = get_brightness_pct(); // % 0-100

    let radius = (RESOLUTION as i32 / 2) + 10;

    let center_x = CENTER;
    let center_y = CENTER;

    // Ring dimensions - foreground is the visible colored arc
    let thickness_fg = 18;
    let stroke_fg = 3u8; // line stroke for smooth drawing
    let radius_fg_outer = radius;
    // Background extends slightly beyond fg for clean clearing
    let radius_bg_outer = radius + 4;
    let radius_bg_inner = (radius - thickness_fg - 6).max(0);

    // Ring angles
    let start = -90.0_f32;
    let end_full = start + 360.0;
    let bg_ring = Rgb565::BLACK;
    let fg_ring = rgb565_from_888(0x9F, 0xFF, 0x4A); // Bright green 0x9FFF4A

    // Compute the exact end angle for a given percentage
    // This function ensures consistent angle calculation everywhere
    let pct_to_ang = |p: u8| -> f32 {
        if p == 100 {
            end_full
        } else {
            start + (p as f32) * 3.6
        }
    };

    // Text box dimensions
    let text_box = (center_x - 70, center_y - 20, center_x + 70, center_y + 20);

    // Fast path: draw directly to panel with incremental updates
    if let Some(co_display) =
        (my_display as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        // Get previous percentage to determine update type
        let prev_pct_opt = get_brightness_last_pct();
        let do_full = prev_pct_opt.is_none();
        let prev_pct = prev_pct_opt.unwrap_or(pct);

        let prev_ang = pct_to_ang(prev_pct);
        let new_ang = pct_to_ang(pct);

        if do_full {
            // Full redraw: clear the ring area only, keep sun icon in center
            // Clamp to exactly 360 degrees (start to end_full) to avoid overflow
            clear_ring_arc_fast(
                co_display, center_x, center_y,
                radius_bg_outer, radius_bg_inner,
                start, end_full,
                bg_ring,
            );
            
            // Draw foreground arc (this handles its own FB update and flush)
            if pct > 0 {
                draw_ring_segment(
                    co_display,
                    center_x,
                    center_y,
                    radius_fg_outer,
                    thickness_fg,
                    stroke_fg,
                    0.0,
                    start,
                    None,
                    new_ang,
                    fg_ring,
                    radius_bg_outer,
                    radius_bg_inner,
                    bg_ring,
                    15.0,
                );
            }
        } else if pct != prev_pct {
            draw_ring_segment(
                co_display,
                center_x,
                center_y,
                radius_fg_outer,
                thickness_fg,
                stroke_fg,
                3.0,
                start,
                Some(prev_ang),
                new_ang,
                fg_ring,
                radius_bg_outer,
                radius_bg_inner,
                bg_ring,
                15.0,
            );
        }

        // Update text - draw into FB, will flush with text box
        let (tx0, ty0, tx1, ty1) = text_box;
        crate::display::FastPanelOps::fill_rect_fb(
            co_display,
            tx0,
            ty0,
            tx1,
            ty1,
            Rgb565::BLACK,
        );
        let pct_buf = format!("{}%", pct);
        draw_text(
            co_display,
            &pct_buf,
            fg_ring,
            None,
            center_x,
            center_y,
            false,
            true,
            Some(&embedded_graphics::mono_font::ascii::FONT_10X20),
        );

        // Save last percentage
        set_brightness_last_pct(Some(pct));

        // Flush only text box
        let fx0 = (tx0.clamp(0, (RESOLUTION - 1) as i32)) & !1;
        let fy0 = (ty0.clamp(0, (RESOLUTION - 1) as i32)) & !1;
        let fx1 = (tx1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32);
        let fy1 = (ty1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32);
        let _ = crate::display::FastPanelOps::flush_rect_even(
            co_display,
            fx0 as u16,
            fy0 as u16,
            fx1 as u16,
            fy1 as u16,
        );
    } else {
        // Fallback: use embedded-graphics path
        let pad = radius_bg_outer + 4;
        let x0 = (center_x - pad).clamp(0, (RESOLUTION - 1) as i32);
        let x1 = (center_x + pad).clamp(0, (RESOLUTION - 1) as i32);
        let y0 = (center_y - pad).clamp(0, (RESOLUTION - 1) as i32);
        let y1 = (center_y + pad).clamp(0, (RESOLUTION - 1) as i32);
        let _ = Rectangle::new(
            Point::new(x0, y0),
            embedded_graphics::prelude::Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(my_display);
        // Generic fallback path uses draw_ring_segment via non optimized path
        // Map percent to sweep angle; use end_full to fully close the ring at 100%.
        let end_ang: f32 = if pct == 100 { end_full } else { start + (pct as f32) * 3.6 };
        draw_ring_segment(
            my_display,
            center_x,
            center_y,
            radius,
            thickness_fg,
            stroke_fg,
            0.0,
            start,
            None,
            end_ang,
            fg_ring,
            radius_bg_outer,
            radius_bg_inner,
            bg_ring,
            15.0,
        );
        let pct_buf = format!("{}%", pct);
        draw_text(
            my_display,
            &pct_buf,
            fg_ring,
            None,
            center_x,
            center_y,
            false,
            true,
            Some(&embedded_graphics::mono_font::ascii::FONT_10X20),
        );
    }
}
