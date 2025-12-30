// Boot animation and staged asset precaching.

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive},
    primitives::{Line, PrimitiveStyle},
    Drawable,
};
use embedded_hal::delay::DelayNs;

use crate::{
    display,
    hal::timer::TimerDelay,
    ui::{self, AssetId, PanelRgb565, RESOLUTION},
};

const BOOT_FRAMES: u32 = 60;
const FRAME_DELAY_MS: u32 = 16;
const OUTER_WIDTH_PX: i32 = 390;
const WAIST_WIDTH_PX: i32 = 115;

fn half_width_for_y(y: i32, mid: i32) -> i32 {
    let waist_half = (WAIST_WIDTH_PX as f32 / 2.0).max(1.0);
    let outer_half = (OUTER_WIDTH_PX as f32 / 2.0).max(waist_half);
    let t = ((y - mid).abs() as f32 / mid.max(1) as f32).min(1.0);
    (waist_half + (outer_half - waist_half) * t + 0.5) as i32
}

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

pub fn run_boot_sequence(display: &mut display::DisplayType<'static>) {
    let mut delay = TimerDelay;
    let assets = [
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

    let mut asset_idx = 0;
    let load_every = (BOOT_FRAMES / assets.len() as u32).max(1);
    let _ = display.fill_rect_solid_no_fb(
        0,
        0,
        RESOLUTION as u16,
        RESOLUTION as u16,
        Rgb565::new(0, 0, 0),
    );

    let mid = (RESOLUTION / 2) as i32;
    draw_hourglass_line(display, mid, mid);
    let mut last_height: i32 = 0;

    for frame in 0..BOOT_FRAMES {
        if asset_idx < assets.len() && frame % load_every == 0 {
            let _ = ui::precache_asset(assets[asset_idx]);
            asset_idx += 1;
        }

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
        delay.delay_ms(FRAME_DELAY_MS);
    }

    while asset_idx < assets.len() {
        let _ = ui::precache_asset(assets[asset_idx]);
        asset_idx += 1;
    }

    if ui::get_cached_asset(AssetId::Logo).is_none() {
        let _ = ui::precache_asset(AssetId::Logo);
    }
    if let Some((buf, w, h)) = ui::get_cached_asset(AssetId::Logo) {
        ui::draw_image_bytes(display, buf, w, h, false, false);
        delay.delay_ms(150);
    }
}
