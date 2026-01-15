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

// Draw a ring arc into the framebuffer, with optional flush, returns affected bbox.
fn draw_ring_arc_fb(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
    stroke: u8,
    do_flush: bool,
) -> Option<(i32, i32, i32, i32)> {
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
        return None;
    }

    // Calculate angular step based on outer radius for smooth appearance.
    let circumference = 2.0 * core::f32::consts::PI * r_outer as f32;
    let pixels_per_degree = circumference / 360.0;
    let step = (2.0 / pixels_per_degree).max(0.5).min(2.0);

    let mut minx = i32::MAX;
    let mut miny = i32::MAX;
    let mut maxx = i32::MIN;
    let mut maxy = i32::MIN;

    let mut draw_spoke = |angle: f32| {
        let rads = angle.to_radians();
        let cos = libm::cosf(rads);
        let sin = libm::sinf(rads);
        let ox = center_x + (cos * r_outer as f32) as i32;
        let oy = center_y + (sin * r_outer as f32) as i32;
        let ix = center_x + (cos * r_inner as f32) as i32;
        let iy = center_y + (sin * r_inner as f32) as i32;

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

    let mut a = ang0;
    while a < ang1 - 0.01 {
        draw_spoke(a);
        a += step;
    }
    draw_spoke(ang1);

    if minx == i32::MAX {
        return None;
    }

    if do_flush {
        let _ = crate::display::FastPanelOps::flush_rect_even(
            my_display,
            minx.clamp(0, (RESOLUTION - 1) as i32) as u16,
            miny.clamp(0, (RESOLUTION - 1) as i32) as u16,
            maxx.clamp(0, (RESOLUTION - 1) as i32) as u16,
            maxy.clamp(0, (RESOLUTION - 1) as i32) as u16,
        );
    }

    Some((minx, miny, maxx, maxy))
}

// Optimized ring arc: draws via framebuffer + flush for smooth output.
// Requires DisplayType; not usable with generic PanelRgb565 paths.
pub fn draw_ring_arc_smooth(
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
    let _ = draw_ring_arc_fb(
        my_display,
        center_x,
        center_y,
        r_outer,
        r_inner,
        ang0_deg,
        ang1_deg,
        color,
        stroke,
        true,
    );
}

// Draw a ring arc into the framebuffer without flushing; returns affected bbox.
pub fn draw_ring_arc_fb_no_flush(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
    stroke: u8,
) -> Option<(i32, i32, i32, i32)> {
    draw_ring_arc_fb(
        my_display,
        center_x,
        center_y,
        r_outer,
        r_inner,
        ang0_deg,
        ang1_deg,
        color,
        stroke,
        false,
    )
}

// Fast rectangular clear for ring arc regions (used for erasing).
// Uses 2x2 block scanning which is fast but produces blocky edges - fine for clearing.
// Used for clearing large ring segments quickly like for a full reset.
pub fn clear_ring_arc_fast(
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
        let cos_a0 = libm::cosf(a0_rad);
        let sin_a0 = libm::sinf(a0_rad);
        let cos_a1 = libm::cosf(a1_rad);
        let sin_a1 = libm::sinf(a1_rad);

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
                let mut ang = libm::atan2f(dy as f32, dx as f32).to_degrees();
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
// Used for shrinking ring segments cleanly and incrementally.
pub fn clear_ring_arc_smooth(
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
    // Validate angles
    if clear_end_deg <= clear_start_deg {
        return;
    }

    // Initialize bbox to first point, NOT to center (to avoid clearing the sun icon).
    let ar0 = clear_start_deg.to_radians();
    let cos0 = libm::cosf(ar0);
    let sin0 = libm::sinf(ar0);
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
        let cos = libm::cosf(ar);
        let sin = libm::sinf(ar);
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
            let cos = libm::cosf(ar);
            let sin = libm::sinf(ar);
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
        let c = libm::cosf(ar);
        let s = libm::sinf(ar);
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
pub fn draw_ring_segment_raw(
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
    if let Some(co_display) = (my_display as &mut dyn core::any::Any)
        .downcast_mut::<crate::display::DisplayType<'static>>()
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
            let ox = center_x + (libm::cosf(ar) * radius as f32) as i32;
            let oy = center_y + (libm::sinf(ar) * radius as f32) as i32;
            let ix = center_x + (libm::cosf(ar) * r_inner as f32) as i32;
            let iy = center_y + (libm::sinf(ar) * r_inner as f32) as i32;
            let _ = Line::new(Point::new(ox, oy), Point::new(ix, iy))
                .into_styled(PrimitiveStyle::with_stroke(color, stroke.max(1) as u32))
                .draw(my_display);
            angle += step;
        }
    }
}

// Draw a ring segment into the framebuffer without flushing; returns affected bbox.
pub fn draw_ring_segment_raw_fb_no_flush(
    my_display: &mut crate::display::DisplayType<'static>,
    center_x: i32,
    center_y: i32,
    radius: i32,
    thickness: i32,
    stroke: u8,
    start_deg: f32,
    end_deg: f32,
    color: Rgb565,
) -> Option<(i32, i32, i32, i32)> {
    let r_inner = radius.saturating_sub(thickness.max(1));
    draw_ring_arc_fb_no_flush(
        my_display,
        center_x,
        center_y,
        radius,
        r_inner,
        start_deg,
        end_deg,
        color,
        stroke.max(1),
    )
}

// delta_deg extends the start angle backward to overdraw for gap-free joins.
// If prev_end_deg is provided, the function handles grow/shrink updates internally.
pub fn draw_ring_segment(
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
            if let Some(co_display) = (my_display as &mut dyn core::any::Any)
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
