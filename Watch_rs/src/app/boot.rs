// Boot animation and staged asset precaching.

use embedded_graphics::{
    Drawable, pixelcolor::Rgb565, prelude::{Point, Primitive, RgbColor}, primitives::{Line, PrimitiveStyle}
};
use embedded_hal::delay::DelayNs;
use esp_hal::system::{CpuControl, Stack};

use crate::{
    display,
    hal::timer::TimerDelay,
    ui::{self, AssetId, PanelRgb565, RESOLUTION},
};
use core::sync::atomic::{AtomicBool, Ordering};
use esp_hal::timer::systimer::{SystemTimer, Unit};

// Boot animation parameters
const BOOT_FRAMES: u32 = 60;
const FRAME_DELAY_MS: u32 = 16;
const OUTER_WIDTH_PX: i32 = 390;
const WAIST_WIDTH_PX: i32 = 115;
const BOOT_ASSETS: [AssetId; 13] = [
    AssetId::Alien1,
    AssetId::Alien2,
    AssetId::Alien3,
    AssetId::Alien4,
    AssetId::Alien5,
    AssetId::Alien6,
    AssetId::Alien7,
    AssetId::Alien8,
    AssetId::Alien9,
    AssetId::Alien10,
    AssetId::SettingsImage,
    AssetId::WatchIcon,
    AssetId::Logo,
];

// Atomic flag to indicate asset loading completion
static ASSETS_DONE: AtomicBool = AtomicBool::new(false);

// Stack for application core during boot asset loading
static mut APP_CORE_STACK: Stack<8192> = Stack::new();

// Calculate half-width of hourglass at given Y position
fn half_width_for_y(y: i32, mid: i32) -> i32 {
    let waist_half = (WAIST_WIDTH_PX as f32 / 2.0).max(1.0);
    let outer_half = (OUTER_WIDTH_PX as f32 / 2.0).max(waist_half);
    let t = ((y - mid).abs() as f32 / mid.max(1) as f32).min(1.0);
    (waist_half + (outer_half - waist_half) * t + 0.5) as i32
}

// Draw a horizontal line of the hourglass at given Y position
fn draw_hourglass_line(disp: &mut impl PanelRgb565, y: i32, mid: i32) {
    if y < 0 || y >= RESOLUTION as i32 {
        return;
    }
    let cx = (RESOLUTION / 2) as i32;
    let half = half_width_for_y(y, mid);
    let green = Rgb565::new(17, 56, 1); // #8BE308
    let style = PrimitiveStyle::with_stroke(green, 1);
    let _ = Line::new(Point::new(cx - half, y), Point::new(cx + half, y))
        .into_styled(style)
        .draw(disp);
}

// Run the boot animation and asset precaching
pub fn run_boot_sequence(
    display: &mut display::DisplayType<'static>,
    cpu_control: &mut CpuControl<'_>,
) {
    let mut delay = TimerDelay;
    let mut asset_idx = 0;

    // Calculate load interval based on total animation time
    let total_anim_ms = (BOOT_FRAMES as u64).saturating_mul(FRAME_DELAY_MS as u64);
    let load_interval_ms = (total_anim_ms / BOOT_ASSETS.len() as u64).max(1);

    // Clear display and reset asset done flag
    ASSETS_DONE.store(false, Ordering::Release);
    let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
        display,
        0,
        0,
        RESOLUTION as u16,
        RESOLUTION as u16,
        Rgb565::BLACK,
    );

    // Start asset loading on application core
    let _app_core_guard = cpu_control
        .start_app_core(unsafe { &mut *core::ptr::addr_of_mut!(APP_CORE_STACK) }, || {
            for &id in BOOT_ASSETS.iter() {
                let _ = ui::precache_asset(id);
            }
            ASSETS_DONE.store(true, Ordering::Release);
        })
        .ok();
    let load_on_main = _app_core_guard.is_none();

    // Begin boot animation
    let mid = (RESOLUTION / 2) as i32;
    draw_hourglass_line(display, mid, mid);
    let mut last_height: i32 = 0;
    let mut last_load_ms: u64 = 0;

    let mut frame: u32 = 0;
    let mut pulse_on = false;
    let mut assets_done_seen = false;
    loop {

        // Handle asset loading on main core if needed, this is a fallback
        if load_on_main {
            let now_ms = {
                let t = SystemTimer::unit_value(Unit::Unit0);
                t.saturating_mul(1000) / SystemTimer::ticks_per_second()
            };
            if asset_idx < BOOT_ASSETS.len()
                && now_ms.saturating_sub(last_load_ms) >= load_interval_ms
            {
                let _ = ui::precache_asset(BOOT_ASSETS[asset_idx]);
                asset_idx += 1;
                last_load_ms = now_ms;
            }
        }

        // Update hourglass animation
        if frame < BOOT_FRAMES {
            let p = frame as f32 / (BOOT_FRAMES.saturating_sub(1)) as f32;
            let target_height = (mid as f32 * p + 0.5) as i32;
            if target_height > last_height {
                for h in (last_height + 1)..=target_height {
                    let y_top = mid - h;
                    let y_bottom = mid + h;
                    draw_hourglass_line(display, y_top, mid);
                    if y_bottom != y_top {
                        draw_hourglass_line(display, y_bottom, mid);
                    }
                }
                last_height = target_height;
            }
        }

        // After initial frames, pulse the hourglass until assets are loaded
        if frame >= BOOT_FRAMES {
            if ASSETS_DONE.load(Ordering::Acquire) {
                assets_done_seen = true;
            }
            if (frame - BOOT_FRAMES) % 20 == 0 {
                pulse_on = !pulse_on;
                let brightness_level = if pulse_on { 255 } else { 64 }; 
                let _ = display.set_brightness(brightness_level);
                if assets_done_seen && pulse_on {
                    break;
                }
            }
        }

        // Wait for next frame
        delay.delay_ms(FRAME_DELAY_MS);
        frame = frame.saturating_add(1);

    }

    // Additional fallback loading on main core if needed
    if load_on_main {
        while asset_idx < BOOT_ASSETS.len() {
            let _ = ui::precache_asset(BOOT_ASSETS[asset_idx]);
            asset_idx += 1;
        }
        ASSETS_DONE.store(true, Ordering::Release);
    } else {
        while !ASSETS_DONE.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }

}
