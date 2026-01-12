// Render the settings page based on the current settings menu state.F

extern crate alloc;

use alloc::format;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{Line, PrimitiveStyle, Rectangle},
    Drawable,
};
use libm::{atan2f, cosf, sinf};

use crate::ui::{
    CENTER, PanelRgb565, RESOLUTION, 
    brightness::{
    brightness_edit_set, get_brightness_last_pct, get_brightness_pct, reset_brightness_last,
    set_brightness_last_pct}, 
    draw::{draw_text, rgb565_from_888}, 
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

// Optimized ring arc: draws via framebuffer + flush for smooth output.
// Requires DisplayType; not usable with generic PanelRgb565 paths.
fn draw_ring_arc_smooth(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32, 
    center_y: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
    stroke: u8,
) {
    // Normalize angles
    let mut ang0 = ang0_deg;
    let mut ang1 = ang1_deg;
    while ang0 < -360.0 {
        ang0 += 360.0;
    }
    while ang1 < ang0 {
        ang1 += 360.0;
    }
    if (ang1 - ang0).abs() < 0.01 {
        return;
    }

    // Calculate angular step based on outer radius for smooth appearance.
    // Smaller step = smoother but more lines. ~1 degree works well for large radii.
    let circumference = 2.0 * core::f32::consts::PI * r_outer as f32;
    let pixels_per_degree = circumference / 360.0;

    // Aim for ~2 pixel spacing between radial lines at outer edge
    let step = (2.0 / pixels_per_degree).max(0.5).min(2.0);

    // Track bounding box of drawn area for flush
    let mut minx = i32::MAX;
    let mut miny = i32::MAX;
    let mut maxx = i32::MIN;
    let mut maxy = i32::MIN;

    // Helper to draw a single radial line and update bbox
    let mut draw_spoke = |angle: f32| {
        let rads = angle.to_radians(); 
        let cos = cosf(rads);
        let sin = sinf(rads);
        let ox = center_x + (cos * r_outer as f32) as i32;
        let oy = center_y + (sin * r_outer as f32) as i32;
        let ix = center_x + (cos * r_inner as f32) as i32;
        let iy = center_y + (sin * r_inner as f32) as i32;

        // Draw the line and get affected area
        if let Some((ax0, ay0, ax1, ay1)) = crate::display::FastPanelOps::draw_line_fb(
            my_display,
            ix,
            iy,
            ox,
            oy,
            color,
            stroke,
        ) {
            minx = minx.min(ax0 as i32);
            miny = miny.min(ay0 as i32);
            maxx = maxx.max(ax1 as i32);
            maxy = maxy.max(ay1 as i32);
        }
    };

    // Draw lines at regular intervals
    let mut a = ang0;
    while a < ang1 - 0.01 {
        draw_spoke(a);
        a += step;
    }
    // Always draw the final line at exactly ang1 to ensure consistent endpoint
    draw_spoke(ang1);

    // Flush the affected region
    if minx != i32::MAX {
        let _ = crate::display::FastPanelOps::flush_rect_even(
            my_display,
            minx.clamp(0, (RESOLUTION - 1) as i32) as u16,
            miny.clamp(0, (RESOLUTION - 1) as i32) as u16,
            maxx.clamp(0, (RESOLUTION - 1) as i32) as u16,
            maxy.clamp(0, (RESOLUTION - 1) as i32) as u16,
        );
    }
}

// Fast rectangular clear for ring arc regions (used for erasing).
// Uses 2x2 block scanning which is fast but produces blocky edges - fine for clearing.
// Used for clearing large ring segments quickly.
fn clear_ring_arc_fast(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
) {
    // Normalize angles
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

    // Compute tight bounding box for the arc
    let arc_span = ang1 - ang0;
    let (minx, miny, maxx, maxy) = if arc_span < 350.0 {
        // Precompute radians
        let a0_rad = ang0.to_radians();
        let a1_rad = ang1.to_radians();

        // Precompute cos/sin for endpoints
        let cos_a0 = cosf(a0_rad);
        let sin_a0 = sinf(a0_rad);
        let cos_a1 = cosf(a1_rad);
        let sin_a1 = sinf(a1_rad);

        // Compute outer and inner edge points at endpoints
        let outer_x0 = cos_a0 * r_outer as f32;
        let outer_y0 = sin_a0 * r_outer as f32;
        let outer_x1 = cos_a1 * r_outer as f32;
        let outer_y1 = sin_a1 * r_outer as f32;
        let inner_x0 = cos_a0 * r_inner as f32;
        let inner_y0 = sin_a0 * r_inner as f32;
        let inner_x1 = cos_a1 * r_inner as f32;
        let inner_y1 = sin_a1 * r_inner as f32;

        // Start with min/max from the four endpoints
        let mut x_min = outer_x0.min(outer_x1).min(inner_x0).min(inner_x1);
        let mut x_max = outer_x0.max(outer_x1).max(inner_x0).max(inner_x1);
        let mut y_min = outer_y0.min(outer_y1).min(inner_y0).min(inner_y1);
        let mut y_max = outer_y0.max(outer_y1).max(inner_y0).max(inner_y1);

        // Check cardinal directions for inclusion
        let check_angle = |target: f32, a0: f32, a1: f32| -> bool {
            // Adjust target angle for wrap-around
            let t = if target < a0 { target + 360.0 } else { target };
            t >= a0 && t <= a1
        };

        // max checks
        if check_angle(0.0, ang0, ang1) {
            x_max = r_outer as f32;
        }
        if check_angle(90.0, ang0, ang1) {
            y_max = r_outer as f32;
        }

        // inverted for min
        if check_angle(180.0, ang0, ang1) {
            x_min = -(r_outer as f32);
        }
        if check_angle(270.0, ang0, ang1) {
            y_min = -(r_outer as f32);
        }

        // Add padding to ensure full coverage
        let pad = 4;
        (
            // Apply padding and clamp to display bounds, ensuring even/odd alignment for efficient clearing
            ((center_x + x_min as i32 - pad).max(0)) & !1,
            ((center_y + y_min as i32 - pad).max(0)) & !1,
            ((center_x + x_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1,
            ((center_y + y_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1,
        )
    } else {
        (
            // Full circle case
            ((center_x - r_outer).max(0)) & !1,
            ((center_y - r_outer).max(0)) & !1,
            ((center_x + r_outer).min((RESOLUTION - 1) as i32)) | 1,
            ((center_y + r_outer).min((RESOLUTION - 1) as i32)) | 1,
        )
    };

    let r2_outer = r_outer * r_outer;
    let r2_inner = r_inner * r_inner;

    // Scan through the bounding box in 2x2 blocks
    for y0 in (miny..=maxy).step_by(2) {

        let y_center = y0 + 1;
        let dy = y_center - center_y;

        // Quick reject rows that are completely outside the outer radius
        if dy * dy > r2_outer {
            continue;
        }

        // Track runs of pixels inside the arc
        let mut run_start: Option<i32> = None;
        let mut run_end: i32 = 0;

        // Scan across the row in 2-pixel steps
        for x0 in (minx..=maxx).step_by(2) {
            let x_center = x0 + 1;
            let dx = x_center - center_x;
            let d2 = dx * dx + dy * dy;

            // Check if the center of this 2x2 block is within the ring segment
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

            if inside_ang {
                // Start a new run if not already started
                if run_start.is_none() {
                    run_start = Some(x0);
                }
                run_end = x0;
            } else if let Some(rs) = run_start {
                // End the current run and draw it
                let width = (run_end - rs + 2) as u16;
                let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
                    my_display,
                    rs as u16,
                    y0 as u16,
                    width,
                    2,
                    color,
                );
                run_start = None;
            }
        }
        if let Some(rs) = run_start {
            // End the current run and draw it
            let width = (run_end - rs + 2) as u16;
            let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
                my_display,
                rs as u16,
                y0 as u16,
                width,
                2,
                color,
            );
        }
    }
}

// Smooth clear of a ring arc, with an optional redraw delta for the remaining tip.
// This keeps a single flush for the clear + rnedraw.
// Used for shrinking ring segments cleanly ad incrementally.
fn clear_ring_arc_smooth(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    r_bg_outer: i32,
    r_bg_inner: i32,
    r_fg_outer: i32,
    r_fg_inner: i32,
    clear_start_deg: f32,
    clear_end_deg: f32,
    tip_end_deg: f32,
    start_limit_deg: f32,
    bg_color: Rgb565,
    fg_color: Rgb565,
    stroke: u8,
    delta_deg: f32,
) {
    if clear_end_deg <= clear_start_deg {
        return;
    }

    // Initialize bbox to first point, NOT to center (to avoid clearing the sun icon).
    let ar0 = clear_start_deg.to_radians();
    let cos0 = cosf(ar0);
    let sin0 = sinf(ar0);
    let init_x = center_x + (cos0 * r_bg_outer as f32) as i32;
    let init_y = center_y + (sin0 * r_bg_outer as f32) as i32;
    let mut bb_minx = init_x;
    let mut bb_maxx = init_x;
    let mut bb_miny = init_y;
    let mut bb_maxy = init_y;

    // Sample the arc to find bounding box (only ring radii, not center).
    let mut a = clear_start_deg;
    while a <= clear_end_deg + 1.0 {
        let ar = a.to_radians();
        let cos = cosf(ar);
        let sin = sinf(ar);
        for r in [r_bg_outer, r_fg_outer, r_fg_inner, r_bg_inner] {
            let px = center_x + (cos * r as f32) as i32;
            let py = center_y + (sin * r as f32) as i32;
            bb_minx = bb_minx.min(px);
            bb_maxx = bb_maxx.max(px);
            bb_miny = bb_miny.min(py);
            bb_maxy = bb_maxy.max(py);
        }
        a += 3.0;
    }
    // Padding for stroke width.
    bb_minx = (bb_minx - 8).max(0);
    bb_miny = (bb_miny - 8).max(0);
    bb_maxx = (bb_maxx + 8).min((RESOLUTION - 1) as i32);
    bb_maxy = (bb_maxy + 8).min((RESOLUTION - 1) as i32);

    // clear the region to background.
    crate::display::FastPanelOps::fill_rect_fb(
        my_display,
        bb_minx,
        bb_miny,
        bb_maxx,
        bb_maxy,
        bg_color,
    );

    // redraw the tip with delta overlap to avoid gaps.
    if tip_end_deg > start_limit_deg {
        let redraw_start = (clear_start_deg - delta_deg).max(start_limit_deg);
        let mut a = redraw_start;
        while a <= tip_end_deg + 0.01 {
            let ar = a.to_radians();
            let cos = cosf(ar);
            let sin = sinf(ar);
            let ox = center_x + (cos * r_fg_outer as f32) as i32;
            let oy = center_y + (sin * r_fg_outer as f32) as i32;
            let ix = center_x + (cos * r_fg_inner as f32) as i32;
            let iy = center_y + (sin * r_fg_inner as f32) as i32;
            let _ = crate::display::FastPanelOps::draw_line_fb(
                my_display,
                ix,
                iy,
                ox,
                oy,
                fg_color,
                stroke,
            );
            a += 0.5;
        }
        // Final spoke at exact tip angle.
        let ar = tip_end_deg.to_radians();
        let c = cosf(ar);
        let s = sinf(ar);
        let ox = center_x + (c * r_fg_outer as f32) as i32;
        let oy = center_y + (s * r_fg_outer as f32) as i32;
        let ix = center_x + (c * r_fg_inner as f32) as i32;
        let iy = center_y + (s * r_fg_inner as f32) as i32;
        let _ = crate::display::FastPanelOps::draw_line_fb(
            my_display,
            ix,
            iy,
            ox,
            oy,
            fg_color,
            stroke,
        );
    }

    // single flush of the affected region.
    let fx0 = (bb_minx & !1) as u16;
    let fy0 = (bb_miny & !1) as u16;
    let fx1 = (bb_maxx | 1).min((RESOLUTION - 1) as i32) as u16;
    let fy1 = (bb_maxy | 1).min((RESOLUTION - 1) as i32) as u16;
    let _ = crate::display::FastPanelOps::flush_rect_even(my_display, fx0, fy0, fx1, fy1);
}

// Generic ring segment: works with any PanelRgb565.
// Uses a fixed step and can fall back to embedded-graphics, so it is less smooth.
fn draw_ring_segment_raw(
    my_display: &mut impl PanelRgb565,
    center_x: i32,
    center_y: i32,
    radius: i32,
    thickness: i32,
    stroke: u8,
    start_deg: f32,
    end_deg: f32,
    color: Rgb565,
) {
    // Fixed step trades smoothness for simplicity (no adaptive spacing here).
    let step = 3.0_f32;
    let r_inner = radius.saturating_sub(thickness.max(1));

    // Fast path: defer to the optimized framebuffer arc.
    if let Some(co_display) =
        (my_display as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        draw_ring_arc_smooth(
            co_display,
            center_x,
            center_y,
            radius,
            r_inner,
            start_deg,
            end_deg,
            color,
            stroke.max(1),
        );
    } else {
        // Fallback: use embedded-graphics path (may flicker more).
        let mut angle = start_deg;
        while angle <= end_deg + 0.1 {
            let ar = angle.to_radians();
            let ox = center_x + (cosf(ar) * radius as f32) as i32;
            let oy = center_y + (sinf(ar) * radius as f32) as i32;
            let ix = center_x + (cosf(ar) * r_inner as f32) as i32;
            let iy = center_y + (sinf(ar) * r_inner as f32) as i32;
            let _ = Line::new(Point::new(ox, oy), Point::new(ix, iy))
                .into_styled(PrimitiveStyle::with_stroke(color, stroke.max(1) as u32))
                .draw(my_display);
            angle += step;
        }
    }
}

// delta_deg extends the start angle backward to overdraw for gap-free joins.
// If prev_end_deg is provided, the function handles grow/shrink updates internally.
fn draw_ring_segment(
    my_display: &mut impl PanelRgb565,
    center_x: i32,
    center_y: i32,
    radius: i32,
    thickness: i32,
    stroke: u8,
    delta_deg: f32,
    start_deg: f32,
    prev_end_deg: Option<f32>,
    end_deg: f32,
    fg_color: Rgb565,
    bg_outer: i32,
    bg_inner: i32,
    bg_color: Rgb565,
    shrink_delta_deg: f32,
) {
    // Full circle: always redraw to ensure a clean closure.
    if end_deg - start_deg >= 359.9 {
        draw_ring_segment_raw(
            my_display,
            center_x,
            center_y,
            radius,
            thickness,
            stroke,
            start_deg,
            end_deg,
            fg_color,
        );
        return;
    }

    if let Some(prev_end) = prev_end_deg {
        if end_deg > prev_end + 0.01 {
            let seg_start = (prev_end - delta_deg).max(start_deg);
            draw_ring_segment_raw(
                my_display,
                center_x,
                center_y,
                radius,
                thickness,
                stroke,
                seg_start,
                end_deg,
                fg_color,
            );
            return;
        }

        if end_deg < prev_end - 0.01 {
            if let Some(co_display) =
                (my_display as &mut dyn core::any::Any)
                    .downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let update_start = if end_deg <= start_deg {
                    start_deg
                } else {
                    (end_deg - 5.0).max(start_deg)
                };
                clear_ring_arc_smooth(
                    co_display,
                    center_x,
                    center_y,
                    bg_outer,
                    bg_inner,
                    radius,
                    radius.saturating_sub(thickness.max(1)),
                    update_start,
                    prev_end,
                    end_deg,
                    start_deg,
                    bg_color,
                    fg_color,
                    stroke.max(1),
                    shrink_delta_deg,
                );
            } else {
                draw_ring_segment_raw(
                    my_display,
                    center_x,
                    center_y,
                    radius,
                    thickness,
                    stroke,
                    end_deg,
                    prev_end,
                    bg_color,
                );
                let tip_start = (end_deg - shrink_delta_deg).max(start_deg);
                draw_ring_segment_raw(
                    my_display,
                    center_x,
                    center_y,
                    radius,
                    thickness,
                    stroke,
                    tip_start,
                    end_deg,
                    fg_color,
                );
            }
            return;
        }

        return;
    }

    draw_ring_segment_raw(
        my_display,
        center_x,
        center_y,
        radius,
        thickness,
        stroke,
        start_deg,
        end_deg,
        fg_color,
    );
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
