// Render the settings page based on the current settings menu state.

extern crate alloc;

use alloc::format;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{Line, PrimitiveStyle, Rectangle},
    Drawable,
};
use libm::{atan2f, cosf, sinf};

use crate::ui::brightness::{
    brightness_edit_set, get_brightness_last_pct, get_brightness_pct, reset_brightness_last,
    set_brightness_last_pct,
};
use crate::ui::draw::{draw_text, rgb565_from_888};
use crate::ui::state::SettingsMenuState;
use crate::ui::{PanelRgb565, CENTER, RESOLUTION};

// Render the settings page based on the current settings menu state.
pub fn render(disp: &mut impl PanelRgb565, settings_state: SettingsMenuState) {
    match settings_state {
        SettingsMenuState::BrightnessPrompt => {
            reset_brightness_last();
            brightness_edit_set(false);
            // Clear the screen, then draw a simple white sun icon with label inside.
            let _ = disp.clear(Rgb565::BLACK);
            let cx = CENTER;
            let cy = CENTER;
            let outer_r = 90;
            let ray_len = 42;
            let ray_thick = 6u8;
            let col = Rgb565::WHITE;
            // Circle + rays using embedded-graphics primitives.
            let _ = embedded_graphics::primitives::Circle::new(
                Point::new(cx - outer_r, cy - outer_r),
                (outer_r * 2) as u32,
            )
            .into_styled(PrimitiveStyle::with_stroke(col, 4))
            .draw(disp);
            for i in 0..8 {
                // Draw rays at 45-degree intervals
                let ang = i as f32 * core::f32::consts::FRAC_PI_4;
                let dx = (cosf(ang) * (outer_r + 4) as f32) as i32;
                let dy = (sinf(ang) * (outer_r + 4) as f32) as i32;
                let tx = cx + dx;
                let ty = cy + dy;
                let rx = (cosf(ang) * (outer_r + ray_len) as f32) as i32 + cx;
                let ry = (sinf(ang) * (outer_r + ray_len) as f32) as i32 + cy;
                let _ = Line::new(Point::new(tx, ty), Point::new(rx, ry))
                    .into_styled(PrimitiveStyle::with_stroke(col, ray_thick as u32))
                    .draw(disp);
            }

            // two layers of text to fit the sun icon
            draw_text(
                disp,
                "Adjust",
                col,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER - 8,
                false,
                false,
                None,
            );
            // second layer for better readability
            draw_text(
                disp,
                "Brightness",
                col,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER + 8,
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
                CENTER,
                CENTER,
                true,
                true,
                None,
            );
        }
    }
}

// Draw an annular arc directly to the panel (no framebuffer update, faster, even-aligned writes)
fn fill_ring_arc_no_fb(
    drv: &mut crate::display::DisplayType<'static>,
    cx: i32,
    cy: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
) -> Option<(i32, i32, i32, i32)> {
    // Normalize angles so ang1 >= ang0 in [0, 360+)
    let mut ang0 = ang0_deg;
    let mut ang1 = ang1_deg;
    while ang0 < 0.0 {
        ang0 += 360.0;
        ang1 += 360.0;
    }
    while ang1 < ang0 {
        ang1 += 360.0;
    }
    if ang1 <= ang0 {
        ang1 = ang0 + 360.0;
    }

    // For small arcs, compute a tighter bounding box based on the arc endpoints
    // This dramatically speeds up incremental updates
    let arc_span = ang1 - ang0;
    let (minx, miny, maxx, maxy) = if arc_span < 350.0 {
        // Compute bbox from arc endpoints for BOTH inner and outer radii
        let a0_rad = ang0.to_radians();
        let a1_rad = ang1.to_radians();

        let cos_a0 = cosf(a0_rad);
        let sin_a0 = sinf(a0_rad);
        let cos_a1 = cosf(a1_rad);
        let sin_a1 = sinf(a1_rad);

        // Start with all 4 arc endpoints (inner/outer at start/end angles)
        let outer_x0 = cos_a0 * r_outer as f32;
        let outer_y0 = sin_a0 * r_outer as f32;
        let outer_x1 = cos_a1 * r_outer as f32;
        let outer_y1 = sin_a1 * r_outer as f32;
        let inner_x0 = cos_a0 * r_inner as f32;
        let inner_y0 = sin_a0 * r_inner as f32;
        let inner_x1 = cos_a1 * r_inner as f32;
        let inner_y1 = sin_a1 * r_inner as f32;

        // Find min/max X/Y among the 4 points
        let mut x_min = outer_x0.min(outer_x1).min(inner_x0).min(inner_x1);
        let mut x_max = outer_x0.max(outer_x1).max(inner_x0).max(inner_x1);
        let mut y_min = outer_y0.min(outer_y1).min(inner_y0).min(inner_y1);
        let mut y_max = outer_y0.max(outer_y1).max(inner_y0).max(inner_y1);

        // Check if arc crosses cardinal directions (0°, 90°, 180°, 270°)
        // and extend bbox accordingly using OUTER radius
        let check_angle = |target: f32, ang0: f32, ang1: f32| -> bool {
            let t = if target < ang0 {
                target + 360.0
            } else {
                target
            };
            t >= ang0 && t <= ang1
        };

        // right
        if check_angle(0.0, ang0, ang1) {
            x_max = r_outer as f32;
        }
        // bottom
        if check_angle(90.0, ang0, ang1) {
            y_max = r_outer as f32;
        } 
        // left
        if check_angle(180.0, ang0, ang1) {
            x_min = -(r_outer as f32);
        }
        // top
        if check_angle(270.0, ang0, ang1) {
            y_min = -(r_outer as f32);
        } 

        // Convert to screen coords with small padding for rounding errors
        let pad = 2;
        let minx = ((cx + x_min as i32 - pad).max(0)) & !1;
        let maxx = ((cx + x_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1;
        let miny = ((cy + y_min as i32 - pad).max(0)) & !1;
        let maxy = ((cy + y_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1;
        (minx, miny, maxx, maxy)
    } else {
        // Full ring - use full bbox
        let minx = ((cx - r_outer).max(0)) & !1;
        let maxx = ((cx + r_outer).min((RESOLUTION - 1) as i32)) | 1;
        let miny = ((cy - r_outer).max(0)) & !1;
        let maxy = ((cy + r_outer).min((RESOLUTION - 1) as i32)) | 1;
        (minx, miny, maxx, maxy)
    };

    // Precompute squared radii
    let r2_outer = r_outer * r_outer;
    let r2_inner = r_inner * r_inner;

    // Bounding box of drawn pixels
    let mut bb: Option<(i32, i32, i32, i32)> = None;

    // Scan rows in 2-pixel bands to satisfy even-write requirement
    for y0 in (miny..=maxy).step_by(2) {
        let y_center = y0 + 1;
        let dy = y_center - cy;
        // Quick reject if outside outer radius
        if dy * dy > r2_outer {
            continue;
        }

        // Scan columns in pairs
        let mut run_start: Option<i32> = None;
        let mut run_end: i32 = 0;

        // Scan two pixels at a time
        for x0 in (minx..=maxx).step_by(2) {
            let x_center = x0 + 1;
            let dx = x_center - cx;
            let d2 = dx * dx + dy * dy;

            // Check if inside ring segment
            let inside_radial = d2 <= r2_outer && d2 >= r2_inner;
            let inside_ang = if inside_radial {
                let mut ang = atan2f(dy as f32, dx as f32).to_degrees();
                // Normalize angle to [0, 360)
                if ang < 0.0 {
                    ang += 360.0;
                }
                if ang < ang0 {
                    ang += 360.0;
                }
                ang >= ang0 && ang <= ang1
            } else {
                false
            };

            // Update run-length encoding
            if inside_ang {
                if run_start.is_none() {
                    run_start = Some(x0);
                }
                run_end = x0;
            } else if let Some(rs) = run_start {
                // End of run - draw it
                let width = (run_end - rs + 2) as u16;
                let _ = drv.fill_rect_solid_no_fb(rs as u16, y0 as u16, width, 2, color);
                bb = Some(match bb {
                    None => (rs, y0, rs + width as i32 - 1, y0 + 1),
                    Some((bx0, by0, bx1, by1)) => (
                        // Update bounding box
                        bx0.min(rs),
                        by0.min(y0),
                        bx1.max(rs + width as i32 - 1),
                        by1.max(y0 + 1),
                    ),
                });
                run_start = None;
            }
        }
        // Flush any remaining run at end of row
        if let Some(rs) = run_start {
            let width = (run_end - rs + 2) as u16;
            let _ = drv.fill_rect_solid_no_fb(rs as u16, y0 as u16, width, 2, color);
            bb = Some(match bb {
                None => (rs, y0, rs + width as i32 - 1, y0 + 1),
                Some((bx0, by0, bx1, by1)) => (
                    // Update bounding box
                    bx0.min(rs),
                    by0.min(y0),
                    bx1.max(rs + width as i32 - 1),
                    by1.max(y0 + 1),
                ),
            });
        }
    }
    bb
}

fn draw_ring_segment(
    disp: &mut impl PanelRgb565,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    start_deg: f32,
    end_deg: f32,
    color: Rgb565,
) {
    // Draw radial lines at intervals to form ring segment
    let step = 3.0_f32;
    let r_inner = radius.saturating_sub(thickness.max(1) - 1);

    // Fast path: draw into FB only and flush once.
    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let mut minx = i32::MAX;
        let mut miny = i32::MAX;
        let mut maxx = i32::MIN;
        let mut maxy = i32::MIN;

        // Draw line and update bbox
        let mut draw_line = |x0: i32, y0: i32, x1: i32, y1: i32| {
            if let Some((ax0, ay0, ax1, ay1)) =
                co.draw_line_fb(x0, y0, x1, y1, color, thickness as u8)
            {
                minx = minx.min(ax0 as i32);
                miny = miny.min(ay0 as i32);
                maxx = maxx.max(ax1 as i32);
                maxy = maxy.max(ay1 as i32);
            }
        };

        // Draw all radial lines
        let mut a = start_deg;
        while a <= end_deg + 0.1 {
            let ar = a.to_radians();
            let ox = cx + (cosf(ar) * radius as f32) as i32;
            let oy = cy + (sinf(ar) * radius as f32) as i32;
            let ix = cx + (cosf(ar) * r_inner as f32) as i32;
            let iy = cy + (sinf(ar) * r_inner as f32) as i32;
            draw_line(ox, oy, ix, iy);
            a += step;
        }

        // Flush affected region
        if minx != i32::MAX {
            let _ = co.flush_rect_even(
                minx.clamp(0, (RESOLUTION - 1) as i32) as u16,
                miny.clamp(0, (RESOLUTION - 1) as i32) as u16,
                maxx.clamp(0, (RESOLUTION - 1) as i32) as u16,
                maxy.clamp(0, (RESOLUTION - 1) as i32) as u16,
            );
        }
    } else {
        // Fallback: use embedded-graphics path (may flicker more).
        let mut a = start_deg;
        while a <= end_deg + 0.1 {
            let ar = a.to_radians();
            let ox = cx + (cosf(ar) * radius as f32) as i32;
            let oy = cy + (sinf(ar) * radius as f32) as i32;
            let ix = cx + (cosf(ar) * r_inner as f32) as i32;
            let iy = cy + (sinf(ar) * r_inner as f32) as i32;
            let _ = Line::new(Point::new(ox, oy), Point::new(ix, iy))
                .into_styled(PrimitiveStyle::with_stroke(color, thickness.max(1) as u32))
                .draw(disp);
            a += step;
        }
    }
}

// Draw the brightness adjustment UI
fn draw_brightness_ui(disp: &mut impl PanelRgb565) {
    // Draw a ring representing brightness percentage and numeric value in center
    let pct = get_brightness_pct(); // % 0-100

    let radius = (RESOLUTION as i32 / 2) + 10;

    let thickness_fg = 20;
    let thickness_bg = thickness_fg + 12;
    let radius_fg_outer = radius;
    let radius_fg_inner = radius - thickness_fg;
    let radius_bg_outer = radius + 2;
    let radius_bg_inner = (radius - thickness_bg - 2).max(0);
    let start = -90.0_f32;
    let end_full = start + 360.0;
    let end_pct = start + (pct as f32) * 3.6;
    let bg_ring = Rgb565::BLACK;
    let fg_ring = rgb565_from_888(0x9F, 0xFF, 0x4A); // Bright green 0x9FFF4A

    let pad = radius_bg_outer + 4;

    // Compute text box for clearing
    let x0 = (CENTER - pad).clamp(0, (RESOLUTION - 1) as i32);
    let x1 = (CENTER + pad).clamp(0, (RESOLUTION - 1) as i32);
    let y0 = (CENTER - pad).clamp(0, (RESOLUTION - 1) as i32);
    let y1 = (CENTER + pad).clamp(0, (RESOLUTION - 1) as i32);
    // Tight text box so we don't wipe nearby graphics.
    let text_box = (CENTER - 70, CENTER - 20, CENTER + 70, CENTER + 20);

    // Fast path: draw directly to panel with incremental updates
    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        // Get previous percentage to determine update type
        let prev_pct_opt = get_brightness_last_pct();
        let do_full = prev_pct_opt.is_none();
        let prev_pct = prev_pct_opt.unwrap_or(pct);

        let prev_ang = start + (prev_pct as f32) * 3.6;
        let new_ang = start + (pct as f32) * 3.6;

        if do_full {
            // Full redraw: background then foreground
            let _ = fill_ring_arc_no_fb(
                co,
                CENTER,
                CENTER,
                radius_bg_outer,
                radius_bg_inner,
                start - 5.0,
                end_full + 5.0,
                bg_ring,
            );
            if pct > 0 {
                let fg_end = if pct == 100 { end_full + 5.0 } else { new_ang };
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_fg_outer,
                    radius_fg_inner,
                    start - 5.0,
                    fg_end,
                    fg_ring,
                );
            }
        } else if pct != prev_pct {
            // Incremental update - use SAME radii for both clear and paint
            // Use the bg radii for everything to ensure consistent ring shape
            let delta = (pct as i32) - (prev_pct as i32);

            if delta > 0 {
                // GROWING: paint the new segment with fg radii
                let fg_start = (prev_ang - 2.0).max(start - 5.0);
                let fg_end = if pct == 100 {
                    end_full + 5.0
                } else {
                    new_ang + 2.0
                };
                // Draw the new segment
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_fg_outer,
                    radius_fg_inner,
                    fg_start,
                    fg_end,
                    fg_ring,
                );
            } else {
                // SHRINKING:
                // First clear the entire area from new_ang to prev_ang using bg radii
                let clear_start = if pct == 0 { start - 5.0 } else { new_ang - 2.0 };
                let clear_end = prev_ang + 5.0;
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_bg_outer,
                    radius_bg_inner,
                    clear_start,
                    clear_end,
                    bg_ring,
                );
                // Repaint the tip AND the outer/inner edges to restore clean boundary
                if pct > 0 {
                    // Repaint a small segment of the foreground to clean up the edge
                    let _ = fill_ring_arc_no_fb(
                        co,
                        CENTER,
                        CENTER,
                        radius_fg_outer,
                        radius_fg_inner,
                        new_ang - 5.0,
                        new_ang + 2.0,
                        fg_ring,
                    );
                }
            }
        }

        // Update text
        let (tx0, ty0, tx1, ty1) = text_box;
        co.fill_rect_fb(tx0, ty0, tx1, ty1, Rgb565::BLACK);
        let pct_buf = format!("{}%", pct);
        draw_text(
            co,
            &pct_buf,
            fg_ring,
            None,
            CENTER,
            CENTER,
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
        let _ = co.flush_rect_even(fx0 as u16, fy0 as u16, fx1 as u16, fy1 as u16);
    } else {
        // Fallback: small clear and redraw (non-panel path).
        let _ = Rectangle::new(
            Point::new(x0, y0),
            embedded_graphics::prelude::Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(disp);
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_bg,
            start,
            end_full,
            bg_ring,
        );
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_bg,
            start,
            end_pct,
            fg_ring,
        );
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_fg,
            start,
            end_pct,
            fg_ring,
        );
        // Text: redraw center text in fallback mode
        let pct_buf = format!("{}%", pct);
        draw_text(
            disp,
            &pct_buf,
            fg_ring,
            None,
            CENTER,
            CENTER - 8,
            false,
            true,
            Some(&embedded_graphics::mono_font::ascii::FONT_10X20),
        );
    }
}
