// Render the watch page based on the current watch state.

extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;

use critical_section::Mutex;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor},
    primitives::{PrimitiveStyle, Rectangle},
    Drawable,
};

use crate::ui::assets::{draw_image_bytes, WATCH_BG_IMAGE};
use crate::ui::draw::{draw_hand_line, draw_text, hand_end, rgb565_from_888};
use crate::ui::state::WatchAppState;
use crate::ui::time::{
    clock_now_hms_f32, current_edit_state, format_clock_hm, hand_cache_mut, take_watch_face_dirty,
};
use crate::ui::{PanelRgb565, CENTER, RESOLUTION};

static LAST_WATCH_STATE: Mutex<RefCell<Option<WatchAppState>>> = Mutex::new(RefCell::new(None));
static LAST_WATCH_EDIT_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static WATCH_BG: Mutex<RefCell<Option<Vec<u8>>>> = Mutex::new(RefCell::new(None));

// Reset watch page state (called when exiting watch page)
pub fn reset_on_exit() {
    critical_section::with(|cs| {
        *LAST_WATCH_STATE.borrow(cs).borrow_mut() = None;
        *WATCH_BG.borrow(cs).borrow_mut() = None; // free background when leaving watch page
        *LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut() = false;
    });
}

// Render the watch page based on the current watch state.
pub fn render(disp: &mut impl PanelRgb565, watch_state: WatchAppState) {
    // If watch mode changed, repaint face and reset cache.
    let should_clear_watch = critical_section::with(|cs| {
        let mut last = LAST_WATCH_STATE.borrow(cs).borrow_mut();
        let changed = *last != Some(watch_state);
        *last = Some(watch_state);
        changed
    });

    if should_clear_watch {
        // Reload background
        if ensure_watch_background_loaded() {
            critical_section::with(|cs| {
                if let Some(bg) = WATCH_BG.borrow(cs).borrow().as_ref() {
                    draw_image_bytes(disp, bg, RESOLUTION, RESOLUTION, false, true);
                }
            });
        }
        crate::ui::time::reset_hand_cache();
    }

    // If time was changed, repaint face and reset cache.
    let face_dirty = take_watch_face_dirty();

    // If dirty, reload background and reset hand cache.
    if face_dirty {
        if ensure_watch_background_loaded() {
            critical_section::with(|cs| {
                if let Some(bg) = WATCH_BG.borrow(cs).borrow().as_ref() {
                    draw_image_bytes(disp, bg, RESOLUTION, RESOLUTION, false, true);
                }
            });
        }
        crate::ui::time::reset_hand_cache();
    }

    match watch_state {
        WatchAppState::Analog => {
            draw_analog_clock(disp);
        }
        WatchAppState::Digital => {
            // Draw either time or edit state
            let edit = current_edit_state();
            let should_clear_after_edit = critical_section::with(|cs| {
                // Check if we were in edit mode last frame but not now
                let mut last = LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut();
                let was = *last;
                let now = edit.is_some();
                *last = now;
                was && !now
            });

            // If were in edit mode last frame but not now, need to clear to bg
            if should_clear_after_edit {
                if ensure_watch_background_loaded() {
                    if let Some(bg) =
                        critical_section::with(|cs| WATCH_BG.borrow(cs).borrow().as_ref().cloned())
                    {
                        draw_image_bytes(disp, &bg, RESOLUTION, RESOLUTION, false, true);
                    }
                }
            }

            // Draw either edit UI or current time
            if let Some(ed) = edit {
                draw_clock_edit(disp, ed);
            } else {
                let mut buf = [b'0'; 5];
                let msg = format_clock_hm(&mut buf);
                draw_text(
                    disp,
                    msg,
                    Rgb565::CYAN,
                    Some(Rgb565::BLACK),
                    CENTER,
                    CENTER,
                    false,
                    true,
                    None,
                );
            }
        }
    }
}

// Ensure the watch background image is loaded into PSRAM
fn ensure_watch_background_loaded() -> bool {
    // Decompress watch background into PSRAM if not already done
    critical_section::with(|cs| {
        if WATCH_BG.borrow(cs).borrow().is_some() {
            return true;
        }

        // Decompress now
        if let Ok(decompressed) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            WATCH_BG_IMAGE,
            (RESOLUTION * RESOLUTION * 2) as usize,
        ) {
            *WATCH_BG.borrow(cs).borrow_mut() = Some(decompressed);
            true
        } else {
            false
        }
    })
}

// Draw the analog clock hands
fn draw_analog_clock(disp: &mut impl PanelRgb565) {
    let center = (RESOLUTION as i32 / 2, RESOLUTION as i32 / 2);
    let cx = center.0;
    let cy = center.1;

    // Current time in fractional hours, minutes, seconds
    let (h, m, s) = clock_now_hms_f32();

    // Angles: 0 deg at 12 o'clock, increasing clockwise
    let sec_ang = (s / 60.0) * 360.0 - 90.0;
    let min_ang = (m / 60.0) * 360.0 - 90.0;
    let hour_ang = (h / 12.0) * 360.0 - 90.0;

    // Hand lengths
    let radius = RESOLUTION as i32 / 2 - 10;
    let sec_len = radius - 10;
    let min_len = radius - 25;
    let hour_len = radius - 50;

    // Compute new endpoints
    let sec_end = hand_end(cx, cy, sec_ang, sec_len);
    let min_end = hand_end(cx, cy, min_ang, min_len);
    let hour_end = hand_end(cx, cy, hour_ang, hour_len);

    // Fast path: draw into FB only and flush once.
    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let (bbox, _) = critical_section::with(|cs| {
            let mut cache = hand_cache_mut(cs);
            let bg_ref = WATCH_BG.borrow(cs).borrow();
            let bgdata = bg_ref.as_ref();

            // Bounding box of old + new hands with padding
            let mut minx = cx;
            let mut miny = cy;
            let mut maxx = cx;
            let mut maxy = cy;
            let mut add_pt = |p: Point, pad: i32| {
                minx = minx.min(p.x - pad);
                miny = miny.min(p.y - pad);
                maxx = maxx.max(p.x + pad);
                maxy = maxy.max(p.y + pad);
            };

            // Add previous hand endpoints
            let sec_stroke = 4;
            let min_stroke = 4;
            let hour_stroke = 4;
            let sec_pad = (sec_stroke * 2).max(6);
            let min_pad = (min_stroke * 2).max(8);
            let hour_pad = (hour_stroke * 2).max(10);

            // Previous points
            if let Some(p) = cache.sec {
                add_pt(p, sec_pad);
            }
            if let Some(p) = cache.min {
                add_pt(p, min_pad);
            }
            if let Some(p) = cache.hour {
                add_pt(p, hour_pad);
            }

            // New points
            add_pt(sec_end, sec_pad);
            add_pt(min_end, min_pad);
            add_pt(hour_end, hour_pad);

            // Center dot padding
            let dot_pad = 22; // covers enlarged center gradient
            add_pt(Point::new(cx, cy), dot_pad);

            // Clear region to background if available, else black
            if let Some(bgdata) = bgdata {
                let bx0 = minx.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let by0 = miny.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let bx1 = maxx.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let by1 = maxy.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let bw = RESOLUTION as usize;
                let w = bx1 - bx0 + 1;
                let h = by1 - by0 + 1;
                let mut buf = alloc::vec::Vec::with_capacity(w * h * 2);
                for row in by0..=by1 {
                    let off = (row * bw + bx0) * 2;
                    buf.extend_from_slice(&bgdata[off..off + w * 2]);
                }
                let _ = co.write_rect_fb(bx0 as u16, by0 as u16, w as u16, h as u16, &buf);
            } else {
                co.fill_rect_fb(minx, miny, maxx, maxy, Rgb565::BLACK);
            }

            // Draw all hands
            // Hour hand
            co.draw_line_fb(
                cx,
                cy,
                hour_end.x,
                hour_end.y,
                Rgb565::WHITE,
                hour_stroke as u8,
            );
            // Minute hand
            co.draw_line_fb(
                cx,
                cy,
                min_end.x,
                min_end.y,
                Rgb565::YELLOW,
                min_stroke as u8,
            );
            // Second hand
            co.draw_line_fb(cx, cy, sec_end.x, sec_end.y, Rgb565::CYAN, sec_stroke as u8);
            // Center dot as solid circle
            let r_outer: i32 = 8;
            let r_outer2: i32 = r_outer * r_outer;
            let c_solid = rgb565_from_888(0x52, 0xC6, 0x6B); // #52C66B
            let x0 = cx - r_outer;
            let y0 = cy - r_outer;
            let x1 = cx + r_outer;
            let y1 = cy + r_outer;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let dx = xx - cx;
                    let dy = yy - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 > r_outer2 {
                        continue;
                    }
                    co.fill_rect_fb(xx, yy, xx, yy, c_solid);
                }
            }

            // Update cache
            cache.sec = Some(sec_end);
            cache.min = Some(min_end);
            cache.hour = Some(hour_end);
            (
                (
                    // Return clamped bbox
                    minx.clamp(0, (RESOLUTION - 1) as i32),
                    miny.clamp(0, (RESOLUTION - 1) as i32),
                    maxx.clamp(0, (RESOLUTION - 1) as i32),
                    maxy.clamp(0, (RESOLUTION - 1) as i32),
                ),
                (),
            )
        });

        // Flush the affected region
        let (minx, miny, maxx, maxy) = bbox;
        let _ = co.flush_rect_even(minx as u16, miny as u16, maxx as u16, maxy as u16);
        return;
    }

    // Fallback: use embedded-graphics path (may flicker more).
    draw_hand_line(disp, cx, cy, sec_end, Rgb565::RED, 2);
    draw_hand_line(disp, cx, cy, min_end, Rgb565::GREEN, 3);
    draw_hand_line(disp, cx, cy, hour_end, Rgb565::BLUE, 4);
}

// Draw the clock edit UI
fn draw_clock_edit(disp: &mut impl PanelRgb565, ed: crate::ui::time::ClockEditState) {
    // Build HH:MM string from digits
    let mut buf = [b'0'; 5];
    buf[0] = b'0' + ed.digits[0];
    buf[1] = b'0' + ed.digits[1];
    buf[2] = b':';
    buf[3] = b'0' + ed.digits[2];
    buf[4] = b'0' + ed.digits[3];

    let msg = core::str::from_utf8(&buf).unwrap_or("00:00");

    let font = &embedded_graphics::mono_font::ascii::FONT_10X20; // largest built-in mono ASCII font available

    // Draw the time (use larger 10x20 font)
    draw_text(
        disp,
        msg,
        Rgb565::CYAN,
        Some(Rgb565::BLACK),
        CENTER,
        CENTER,
        false,
        true,
        Some(font),
    );

    // Underline the active digit only (skip the colon)
    let char_w = font.character_size.width as i32;
    let char_h = font.character_size.height as i32;
    let chars_total = 5;
    let box_w = char_w * chars_total;
    let start_x = CENTER - box_w / 2;
    let base_y = CENTER + char_h / 2 + 2;
    let idx = ed.idx.min(3) as i32;
    let visual_idx = if idx >= 2 { idx + 1 } else { idx }; // skip colon slot
    let underline_x = start_x + visual_idx * char_w;

    // Draw underline rectangle
    let rect = Rectangle::new(
        Point::new(underline_x, base_y),
        embedded_graphics::prelude::Size::new(char_w as u32, 2),
    );
    rect.into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN))
        .draw(disp)
        .ok();
}
