// Asset management: loading, caching, and drawing images.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::RefCell;

use critical_section::Mutex;
use embedded_graphics::{
    image::{Image, ImageRawBE},
    pixelcolor::Rgb565,
    prelude::{Point, RgbColor},
    Drawable,
};
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::ui::{PanelRgb565, RESOLUTION};

// Feature-selected image dimensions (adjust OLED to 466 if you have 466×466 assets)
pub const MAX_IMG_W: u32 = 466;
pub const MAX_IMG_H: u32 = 466;

pub const IMG_W: u32 = 308;
pub const IMG_H: u32 = 374;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AssetId {
    Alien1,
    Alien2,
    Alien3,
    Alien4,
    Alien5,
    Alien6,
    Alien7,
    Alien8,
    Alien9,
    Alien10,
    Logo,
    InfoPage,
    SettingsImage,
    WatchIcon,
}

#[derive(Copy, Clone)]
struct AssetSlot {
    data: Option<&'static [u8]>,
    w: u32,
    h: u32,
}

// Number of asset slots
const ASSET_MAX: usize = 14;

macro_rules! res {
    () => {
        "308x374"
    };
} // just a convenience macro for asset paths, a lot have this resolution

// Feature-picked assets (compressed, zlib)
pub(crate) static ALIEN1_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien1_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN2_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien2_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN3_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien3_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN4_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien4_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN5_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien5_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN6_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien6_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN7_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien7_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN8_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien8_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN9_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien9_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN10_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/alien10_", res!(), "_rgb565_be.raw.zlib"));
pub(crate) static ALIEN_LOGO: &[u8] = include_bytes!(concat!(
    "../assets/omnitrix_logo_466x466_rgb565_be.raw.zlib"
));
pub(crate) static INFO_PAGE_IMAGE: &[u8] =
    include_bytes!(concat!("../assets/debug_image3_466x466_rgb565_be.raw.zlib"));
pub(crate) static SETTINGS_IMAGE: &[u8] =
    include_bytes!("../assets/settings_image_400x344_rgb565_be.raw.zlib");
pub(crate) static WATCH_ICON_IMAGE: &[u8] =
    include_bytes!("../assets/watch_icon_316x316_rgb565_be.raw.zlib");
pub(crate) static WATCH_BG_IMAGE: &[u8] =
    include_bytes!("../assets/watch_background_466x466_rgb565_be.raw.zlib");

// Generic asset cache
static ASSETS: Mutex<RefCell<[AssetSlot; ASSET_MAX]>> = Mutex::new(RefCell::new(
    [AssetSlot {
        data: None,
        w: 0,
        h: 0,
    }; ASSET_MAX],
));

// Clear all cached assets from PSRAM
pub fn clear_cache() {
    critical_section::with(|cs| {
        let mut assets = ASSETS.borrow(cs).borrow_mut();
        for slot in assets.iter_mut() {
            slot.data = None;
            slot.w = 0;
            slot.h = 0;
        }
    });
}

// Map asset id to cache slot index, dimensions, and compressed blob
fn asset_meta(id: AssetId) -> (usize, u32, u32, &'static [u8]) {
    match id {
        // index, width, height, blob
        AssetId::Alien1 => (0, 308, 374, ALIEN1_IMAGE),
        AssetId::Alien2 => (1, 308, 374, ALIEN2_IMAGE),
        AssetId::Alien3 => (2, 308, 374, ALIEN3_IMAGE),
        AssetId::Alien4 => (3, 308, 374, ALIEN4_IMAGE),
        AssetId::Alien5 => (4, 308, 374, ALIEN5_IMAGE),
        AssetId::Alien6 => (5, 308, 374, ALIEN6_IMAGE),
        AssetId::Alien7 => (6, 308, 374, ALIEN7_IMAGE),
        AssetId::Alien8 => (7, 308, 374, ALIEN8_IMAGE),
        AssetId::Alien9 => (8, 308, 374, ALIEN9_IMAGE),
        AssetId::Alien10 => (9, 308, 374, ALIEN10_IMAGE),
        AssetId::Logo => (10, 466, 466, ALIEN_LOGO),
        AssetId::InfoPage => (11, 466, 466, INFO_PAGE_IMAGE),
        AssetId::SettingsImage => (12, 400, 344, SETTINGS_IMAGE),
        AssetId::WatchIcon => (13, 316, 316, WATCH_ICON_IMAGE),
    }
}

// Map OmnitrixState to AssetId
pub fn asset_id_for_state(s: crate::ui::state::OmnitrixState) -> AssetId {
    match s {
        crate::ui::state::OmnitrixState::Alien1 => AssetId::Alien1,
        crate::ui::state::OmnitrixState::Alien2 => AssetId::Alien2,
        crate::ui::state::OmnitrixState::Alien3 => AssetId::Alien3,
        crate::ui::state::OmnitrixState::Alien4 => AssetId::Alien4,
        crate::ui::state::OmnitrixState::Alien5 => AssetId::Alien5,
        crate::ui::state::OmnitrixState::Alien6 => AssetId::Alien6,
        crate::ui::state::OmnitrixState::Alien7 => AssetId::Alien7,
        crate::ui::state::OmnitrixState::Alien8 => AssetId::Alien8,
        crate::ui::state::OmnitrixState::Alien9 => AssetId::Alien9,
        crate::ui::state::OmnitrixState::Alien10 => AssetId::Alien10,
    }
}

// Pre-cache a compressed asset into PSRAM
pub fn precache_asset(id: AssetId) -> bool {
    let (idx, w, h, blob) = asset_meta(id);
    let need = (w * h * 2) as usize;
    critical_section::with(|cs| {
        if ASSETS.borrow(cs).borrow()[idx].data.is_some() {
            return true;
        }
        if let Ok(tmp) = decompress_to_vec_zlib_with_limit(blob, need) {
            // If decompression worked and got the expected size, store in cache
            if tmp.len() == need {
                let leaked: &'static mut [u8] = Box::leak(tmp.into_boxed_slice());
                ASSETS.borrow(cs).borrow_mut()[idx] = AssetSlot {
                    data: Some(leaked as &'static [u8]),
                    w,
                    h,
                };
                return true;
            }
        }
        false
    })
}

// Pre-cache all (call once at boot)
pub fn precache_all() -> usize {
    let mut ok = 0;
    for id in [
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
        AssetId::Logo,
        AssetId::SettingsImage,
        AssetId::WatchIcon,
    ] {
        if precache_asset(id) {
            ok += 1;
        } else {
            break;
        }
    }
    ok
}

// Get cached bytes and dims
pub fn get_cached_asset(id: AssetId) -> Option<(&'static [u8], u32, u32)> {
    let (idx, _, _, _) = asset_meta(id);
    critical_section::with(|cs| {
        let slot = ASSETS.borrow(cs).borrow()[idx];
        slot.data.map(|d| (d, slot.w, slot.h))
    })
}

// Draw from already-decompressed bytes (used by cache on OLED)
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
                let _ = co.fill_rect_solid_no_fb(
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

    // Try fast raw blit if this really is the CO5300 driver (DMA or non-DMA alias).
    // The display backend re-exports its concrete type as display::DisplayType.
    if let Some(co) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let res = if update_fb {
            co.blit_rect_be_fast(x as u16, y as u16, w as u16, h as u16, bytes)
        } else {
            co.blit_rect_be_fast_no_fb(x as u16, y as u16, w as u16, h as u16, bytes)
        };
        if let Err(e) = res {
            esp_println::println!("fast blit failed: {:?}; fallback", e);
            let raw = ImageRawBE::<Rgb565>::new(bytes, w);
            let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
        }
    } else {
        let raw = ImageRawBE::<Rgb565>::new(bytes, w);
        let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
    }
}
