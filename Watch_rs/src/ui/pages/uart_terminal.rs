// UART terminal page renderer.
//
// Displays up to 8 lines of 16 chars each in a 320x320 box centered on the
// 466x466 display (top-left corner of box at x=73, y=73).
//
// Optimisation strategy:
//  - Initial render (entering page): clear full screen to black, draw all lines,
//    flush only the 320x320 terminal region.
//  - Incremental render (new lines arrived, no scroll yet): draw only the new
//    rows and flush just those rows.
//  - Scroll render (buffer was full, oldest line evicted): clear 320x320 box,
//    redraw all visible lines, flush 320x320.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    Pixel,
    pixelcolor::Rgb565,
    prelude::{OriginDimensions, Point, RgbColor, Size},
    text::{Baseline, Text},
    Drawable,
};

use crate::app::state::{terminal_line_count, terminal_take_dirty, terminal_with_lines, TERM_ROWS};
use crate::ui::PanelRgb565;

// Terminal box geometry (centered on 466x466 display)
pub const TERM_X: i32 = 73;
pub const TERM_Y: i32 = 73;
pub const TERM_W: u16 = 320;
pub const TERM_H: u16 = 320;
const FONT_H: i32 = 20;
const TEXT_SCALE: i32 = 2;
const LINE_H: i32 = FONT_H * TEXT_SCALE;

// Tracks how many lines were drawn in the last render cycle so we can do
// incremental updates when possible.
static LAST_DRAWN_COUNT: AtomicU8 = AtomicU8::new(0);
// Set to false when the page is first entered so the initial render clears the screen.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn reset_on_exit() {
    INITIALIZED.store(false, Ordering::Relaxed);
    LAST_DRAWN_COUNT.store(0, Ordering::Relaxed);
}

pub fn render(disp: &mut impl PanelRgb565) {
    // Always consume the dirty flag so it doesn't retrigger.
    let _ = terminal_take_dirty();

    let is_initial = !INITIALIZED.load(Ordering::Relaxed);
    let total = terminal_line_count();
    let last = LAST_DRAWN_COUNT.load(Ordering::Relaxed) as usize;

    // Determine render mode:
    //  - initial: first time on this page: full black + redraw
    //  - incremental: new lines added, no scroll: draw only new rows
    //  - scroll: buffer was full, lines shifted: clear box + redraw all
    let scroll_occurred = total == TERM_ROWS && last == TERM_ROWS;
    let new_lines = if total > last && !scroll_occurred { total - last } else { 0 };

    if is_initial {
        // Clear entire 466x466 display to black before showing anything.
        fast_fill_fb(disp, 0, 0, 466, 466, Rgb565::BLACK);
        INITIALIZED.store(true, Ordering::Relaxed);
        // Draw all buffered lines (may be empty on first boot).
        draw_all_lines(disp, total);
        flush_region(disp, TERM_X as u16, TERM_Y as u16, TERM_X as u16 + TERM_W, TERM_Y as u16 + TERM_H);
    } else if scroll_occurred || new_lines == 0 {
        // Scroll happened or nothing changed (shouldn't normally call render then,
        // but handle gracefully): clear box and redraw all.
        fast_fill_fb(disp, TERM_X as u16, TERM_Y as u16, TERM_W, TERM_H, Rgb565::BLACK);
        draw_all_lines(disp, total);
        flush_region(disp, TERM_X as u16, TERM_Y as u16, TERM_X as u16 + TERM_W, TERM_Y as u16 + TERM_H);
    } else {
        // Incremental: draw only the `new_lines` rows at the bottom.
        let first_new_row = last;
        draw_lines_range(disp, first_new_row, total);
        // Flush just the rows we drew.
        let y0 = (TERM_Y + first_new_row as i32 * LINE_H) as u16;
        let y1 = (TERM_Y + total as i32 * LINE_H) as u16;
        flush_region(disp, TERM_X as u16, y0, TERM_X as u16 + TERM_W, y1);
    }

    LAST_DRAWN_COUNT.store(total as u8, Ordering::Relaxed);
}

// Draw all currently buffered lines from row 0..total.
fn draw_all_lines(disp: &mut impl PanelRgb565, total: usize) {
    terminal_with_lines(|lines| {
        let count = lines.len().min(TERM_ROWS);
        // If buffer has more lines than TERM_ROWS, show the last TERM_ROWS.
        let skip = if lines.len() > TERM_ROWS { lines.len() - TERM_ROWS } else { 0 };
        for (row, line) in lines.iter().skip(skip).enumerate() {
            draw_scaled_line(disp, line.as_str(), row);
        }
        let _ = total; // suppress unused warning
        let _ = count;
    });
}

// Draw lines from `start_row` (inclusive) to `end_row` (exclusive).
fn draw_lines_range(disp: &mut impl PanelRgb565, start_row: usize, end_row: usize) {
    terminal_with_lines(|lines| {
        let skip = if lines.len() > TERM_ROWS { lines.len() - TERM_ROWS } else { 0 };
        for (row, line) in lines.iter().skip(skip).enumerate() {
            if row >= start_row && row < end_row {
                draw_scaled_line(disp, line.as_str(), row);
            }
        }
    });
}

fn draw_scaled_line(disp: &mut impl PanelRgb565, line: &str, row: usize) {
    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let mut target = Scale2xFb { display: co };
        Text::with_baseline(
            line,
            Point::new(0, row as i32 * FONT_H),
            style,
            Baseline::Top,
        )
        .draw(&mut target)
        .ok();
    } else {
        Text::with_baseline(
            line,
            Point::new(TERM_X, TERM_Y + row as i32 * LINE_H),
            style,
            Baseline::Top,
        )
        .draw(disp)
        .ok();
    }
}

struct Scale2xFb<'a> {
    display: &'a mut crate::display::DisplayType<'static>,
}

impl OriginDimensions for Scale2xFb<'_> {
    fn size(&self) -> Size {
        Size::new(
            (TERM_W as i32 / TEXT_SCALE) as u32,
            (TERM_H as i32 / TEXT_SCALE) as u32,
        )
    }
}

impl DrawTarget for Scale2xFb<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(p, color) in pixels {
            if p.x < 0 || p.y < 0 {
                continue;
            }
            let x = TERM_X + p.x * TEXT_SCALE;
            let y = TERM_Y + p.y * TEXT_SCALE;
            crate::display::FastPanelOps::fill_rect_fb(
                self.display,
                x,
                y,
                x + TEXT_SCALE - 1,
                y + TEXT_SCALE - 1,
                color,
            );
        }
        Ok(())
    }
}

// Fast single-color fill that also mirrors into the framebuffer. The terminal
// later flushes from FB, so a no-FB clear would re-send stale boot pixels.
fn fast_fill_fb(disp: &mut impl PanelRgb565, x: u16, y: u16, w: u16, h: u16, color: Rgb565) {
    use core::any::Any;
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        crate::display::FastPanelOps::fill_rect_fb(
            co,
            x as i32,
            y as i32,
            x.saturating_add(w.saturating_sub(1)) as i32,
            y.saturating_add(h.saturating_sub(1)) as i32,
            color,
        );
        let _ = crate::display::FastPanelOps::flush_rect_even(
            co,
            x,
            y,
            x.saturating_add(w.saturating_sub(1)),
            y.saturating_add(h.saturating_sub(1)),
        );
    } else {
        let _ = disp.clear(color);
    }
}

// Flush a rectangular region to the hardware display.
fn flush_region(disp: &mut impl PanelRgb565, x0: u16, y0: u16, x1: u16, y1: u16) {
    use core::any::Any;
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let _ = crate::display::FastPanelOps::flush_rect_even(co, x0, y0, x1, y1);
    }
}
