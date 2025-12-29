// Render the Transform page with animated DNA helix

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{PrimitiveStyle, Rectangle},
    Drawable,
};
use libm::{cosf, sinf};

use crate::ui::draw::rgb565_from_888;
use crate::ui::time::clock_now_seconds_f32;
use crate::ui::{PanelRgb565, CENTER, RESOLUTION};

// Render the Transform page with animated DNA helix
pub fn render(disp: &mut impl PanelRgb565, last_active: &mut bool) {
    // On first entry into Transform dialog, hard clear the whole screen.
    if !*last_active {
        *last_active = true;
        if let Some(co) =
            (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
        {
            let _ =
                co.fill_rect_solid_no_fb(0, 0, RESOLUTION as u16, RESOLUTION as u16, Rgb565::BLACK);
            co.fill_rect_fb(
                0,
                0,
                (RESOLUTION - 1) as i32,
                (RESOLUTION - 1) as i32,
                Rgb565::BLACK,
            );
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }
    draw_transform_overlay(disp);
}

fn draw_transform_overlay(disp: &mut impl PanelRgb565) {
    // DNA-like helix animation with depth sorting for proper 3D illusion
    let t = clock_now_seconds_f32() * 1.6; // slower rotation for better 3D illusion
    let amp_max = (RESOLUTION as f32) * 0.26; // max amplitude based on panel size
    let step = 16; // slightly tighter spacing for smoother curve
    let cx = CENTER;
    let y_start = 12; // avoid top edge
    let y_end = RESOLUTION as i32 - 12; // avoid bottom edge

    // Front/back color pairs with more contrast for depth
    let strand_a_front = rgb565_from_888(0xC0, 0xFF, 0x70); // brighter front
    let strand_a_back = rgb565_from_888(0x40, 0x90, 0x10); // darker back
    let strand_b_front = rgb565_from_888(0xA8, 0xFF, 0x50);
    let strand_b_back = rgb565_from_888(0x38, 0x80, 0x08);
    let rung_front = rgb565_from_888(0xB0, 0xFF, 0x60);
    let rung_back = rgb565_from_888(0x50, 0x90, 0x18);

    // Base thickness values - will be modulated by depth
    let strand_thick_base = 6u8;
    let rung_thick = 3u8;

    // Bounding box for the helix drawing (reuse for clear/flush).
    let pad = (amp_max as i32 + 20).min(CENTER);
    let x0 = (cx - pad).clamp(0, (RESOLUTION - 1) as i32);
    let x1 = (cx + pad).clamp(0, (RESOLUTION - 1) as i32);
    let y0 = (y_start - 8).clamp(0, (RESOLUTION - 1) as i32);
    let y1 = (y_end + 8).clamp(0, (RESOLUTION - 1) as i32);

    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        // Clear only the helix region in the framebuffer each frame.
        co.fill_rect_fb(x0, y0, x1, y1, Rgb565::BLACK);

        // Collect strand segments for depth-sorted drawing
        // (y_pos, depth, is_strand_a, prev_point, curr_point)
        let mut segments: heapless::Vec<(i32, f32, bool, Point, Point), 64> = heapless::Vec::new();

        // Collect rungs with depth info for proper front/back coloring
        // (y_pos, depth, point_a, point_b, is_front)
        let mut rungs: heapless::Vec<(i32, f32, Point, Point, bool), 32> = heapless::Vec::new();

        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;

        // Generate strand points
        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            
            // Calculate phase, amplitude, and offsets
            let phase = t + (i as f32) * 0.32;
            let amp = amp_max * 0.75;

            let off_a = (sinf(phase) * amp) as i32;
            let off_b = -off_a;

            // Current strand points
            let xa = cx + off_a;
            let xb = cx + off_b;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);

            // Depth value: cosf gives z-depth (-1 = back, +1 = front)
            let depth_a = cosf(phase);

            if let (Some(pa_prev), Some(pb_prev)) = (prev_a, prev_b) {
                let prev_phase = t + ((i - 1) as f32) * 0.32;
                let avg_depth_a = (depth_a + cosf(prev_phase)) / 2.0;
                let avg_depth_b = -avg_depth_a;

                let _ = segments.push((y, avg_depth_a, true, pa_prev, pa));
                let _ = segments.push((y, avg_depth_b, false, pb_prev, pb));
            }

            // Draw rungs at fixed Y intervals
            if i % 3 == 1 {
                // Rung visibility based on rotation: when strands are at edges (|sinf| high),
                // the rung is facing us or away. When |sinf| is low, rung is on the side.
                // Use cosf to determine if rung faces front or back
                let rung_facing_front = cosf(phase).abs() < 0.7; // rung visible when strands near edges
                let rung_depth = if rung_facing_front { 0.1 } else { -0.5 };
                let _ = rungs.push((y, rung_depth, pa, pb, rung_facing_front));
            }

            prev_a = Some(pa);
            prev_b = Some(pb);
        }

        // Sort strands by depth (back-to-front)
        for i in 0..segments.len() {
            for j in 0..segments.len().saturating_sub(1 + i) {
                if segments[j].1 > segments[j + 1].1 {
                    segments.swap(j, j + 1);
                }
            }
        }

        // Sort rungs by depth too
        for i in 0..rungs.len() {
            for j in 0..rungs.len().saturating_sub(1 + i) {
                if rungs[j].1 > rungs[j + 1].1 {
                    rungs.swap(j, j + 1);
                }
            }
        }

        // Interleave drawing: back rungs, back strands, front rungs, front strands
        // Draw back rungs first
        for &(_y, depth, pa, pb, is_front) in rungs.iter() {
            if depth < 0.0 {
                let col = if is_front { rung_front } else { rung_back };
                let _ = co.draw_line_fb(pa.x, pa.y, pb.x, pb.y, col, rung_thick);
            }
        }

        // Draw sorted strand segments (back ones first due to sorting)
        for &(_y, depth, is_a, p_prev, p_curr) in segments.iter() {
            // Modulate thickness based on depth for 3D effect
            let depth_factor = (depth + 1.0) / 2.0;
            let strand_thick = ((strand_thick_base as f32) * (0.5 + 0.7 * depth_factor)) as u8;
            let strand_thick = strand_thick.max(3).min(9);

            let front_side = depth >= 0.0;

            // Choose colors based on strand and front/back
            let (col_main, col_shadow) = if is_a {
                if front_side {
                    (strand_a_front, rgb565_from_888(0x70, 0xB0, 0x30))
                } else {
                    (strand_a_back, rgb565_from_888(0x28, 0x60, 0x08))
                }
            } else {
                if front_side {
                    (strand_b_front, rgb565_from_888(0x60, 0xA0, 0x28))
                } else {
                    (strand_b_back, rgb565_from_888(0x20, 0x50, 0x04))
                }
            };

            // Draw shadow (thicker, darker) then main strand
            let _ = co.draw_line_fb(
                p_prev.x,
                p_prev.y,
                p_curr.x,
                p_curr.y,
                col_shadow,
                strand_thick + 2,
            );

            // Draw main strand
            let _ = co.draw_line_fb(
                p_prev.x,
                p_prev.y,
                p_curr.x,
                p_curr.y,
                col_main,
                strand_thick,
            );
        }

        // Draw front rungs last (on top of strands)
        for &(_y, depth, pa, pb, is_front) in rungs.iter() {
            if depth >= 0.0 {
                let col = if is_front { rung_front } else { rung_back };
                let _ = co.draw_line_fb(pa.x, pa.y, pb.x, pb.y, col, rung_thick);
            }
        }

        // Flush only the helix region to avoid needless panel churn.
        let _ = co.flush_rect_even(x0 as u16, y0 as u16, x1 as u16, y1 as u16);
    } else {
        // Fallback path using embedded-graphics primitives.
        let strand_thick = strand_thick_base; // use base thickness for fallback
        let _ = Rectangle::new(
            Point::new(x0, y0),
            embedded_graphics::prelude::Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(disp);
        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;

        // Draw helix strands
        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            let phase = t + (i as f32) * 0.35;
            let amp = amp_max * 0.75;
            let off = (sinf(phase) * amp) as i32;
            let xa = cx + off;
            let xb = cx - off;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);
            let front_side = sinf(phase) >= 0.0;

            // Choose colors based on front/back
            let col_a = if front_side {
                strand_a_front
            } else {
                strand_a_back
            };
            let col_b = if front_side {
                strand_b_back
            } else {
                strand_b_front
            };
            let col_a_sh = rgb565_from_888(
                (col_a.r().saturating_mul(3) / 4) as u8,
                (col_a.g().saturating_mul(3) / 4) as u8,
                (col_a.b().saturating_mul(3) / 4) as u8,
            );
            let col_b_sh = rgb565_from_888(
                (col_b.r().saturating_mul(3) / 4) as u8,
                (col_b.g().saturating_mul(3) / 4) as u8,
                (col_b.b().saturating_mul(3) / 4) as u8,
            );

            // Connect strands smoothly
            if let Some(p) = prev_a {
                let _ = embedded_graphics::primitives::Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(col_a_sh, strand_thick.into()))
                    .draw(disp);
                let _ = embedded_graphics::primitives::Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_a,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }

            // Connect strands smoothly
            if let Some(p) = prev_b {
                let _ = embedded_graphics::primitives::Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(col_b_sh, strand_thick.into()))
                    .draw(disp);
                let _ = embedded_graphics::primitives::Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_b,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }

            // Curved rung: bend slightly using a midpoint offset for a faux spin effect.
            let mid_phase = phase + core::f32::consts::FRAC_PI_2;
            let mid_bend = (sinf(mid_phase) * amp * 0.18) as i32;
            let mid_x = cx + mid_bend;
            let mid_y = y + step as i32 / 2;
            let pm = Point::new(mid_x, mid_y);
            let col_rung = if front_side { rung_front } else { rung_back };

            // Draw two segments to form a bent rung
            let _ = embedded_graphics::primitives::Line::new(pa, pm)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);
            let _ = embedded_graphics::primitives::Line::new(pm, pb)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);

            prev_a = Some(pa);
            prev_b = Some(pb);
        }
    }
}
