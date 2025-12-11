//! UI state management and display rendering module.
//!
//! This module provides:
//! - The `UiState` enum and its navigation methods (`next`, `prev`, etc.)
//! - The `update_ui` function to render the current UI state to the display
//! - Drawing helpers for text, shapes, and layout
//!
//! Designed for use with embedded-graphics, mipidsi, and ESP-HAL display drivers.
//! All drawing is centered on a 240x240 display, but can be adapted for other sizes.

extern crate alloc;
use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;

use esp_backtrace as _;

// Embedded-graphics, a ton are unused but this is a work in progress
use embedded_graphics::{
    draw_target::DrawTarget,
    image::{Image, ImageRawBE},
    mono_font::{
        ascii::FONT_10X20,
        MonoFont, MonoTextStyleBuilder,
    },
    pixelcolor::Rgb565,
    prelude::{OriginDimensions, Point, Primitive, RgbColor, Size},
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
    Drawable,
};
use esp_hal::timer::systimer::{SystemTimer, Unit};
use libm::{cosf, sinf};

use core::any::Any;
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

// Make a lightweight trait bound we’ll use for the factory’s return type.
pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}

// Display configuration, (0,0) is top-left corner

pub const RESOLUTION: u32 = 466;

pub const CENTER: i32 = (RESOLUTION / 2) as i32;

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
}

#[derive(Copy, Clone)]
struct AssetSlot {
    data: Option<&'static [u8]>,
    w: u32,
    h: u32,
}

// Number of asset slots
const ASSET_MAX: usize = 12;

macro_rules! res {
    () => {
        "308x374"
    };
} // just a convenience macro for asset paths, a lot have this resolution

// Custom colors
#[allow(dead_code)]
const OMNI_LIME: Rgb565 = Rgb565::new(0x11, 0x38, 0x01); // #8BE308

// Feature-picked assets (compressed, zlib)
static ALIEN1_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien1_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN2_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien2_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN3_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien3_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN4_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien4_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN5_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien5_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN6_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien6_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN7_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien7_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN8_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien8_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN9_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien9_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN10_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien10_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN_LOGO: &[u8] =
    include_bytes!(concat!("assets/omnitrix_logo_466x466_rgb565_be.raw.zlib"));
static INFO_PAGE_IMAGE: &[u8] =
    include_bytes!(concat!("assets/debug_image3_466x466_rgb565_be.raw.zlib"));
static WATCH_BG_IMAGE: &[u8] =
    include_bytes!("assets/watch_background_466x466_rgb565_be.raw.zlib");

// Generic asset cache
static ASSETS: Mutex<RefCell<[AssetSlot; ASSET_MAX]>> = Mutex::new(RefCell::new(
    [AssetSlot {
        data: None,
        w: 0,
        h: 0,
    }; ASSET_MAX],
));

// Page kind tracker for optimization
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PageKind {
    Main,
    Settings,
    Omnitrix,
    EasterEgg,
    Watch,
}
static LAST_PAGE_KIND: Mutex<RefCell<Option<PageKind>>> = Mutex::new(RefCell::new(None));

// Omnitrix transform active tracker
static LAST_OMNI_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

// Navigation history management
static NAV_HISTORY: Mutex<RefCell<Vec<Page>>> = Mutex::new(RefCell::new(Vec::new()));
static LAST_WATCH_STATE: Mutex<RefCell<Option<WatchAppState>>> = Mutex::new(RefCell::new(None));
static CLOCK_EDIT: Mutex<RefCell<Option<ClockEditState>>> = Mutex::new(RefCell::new(None));
static LAST_WATCH_EDIT_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static HAND_CACHE: Mutex<RefCell<HandCache>> = Mutex::new(RefCell::new(HandCache::new()));
static WATCH_BG: Mutex<RefCell<Option<alloc::vec::Vec<u8>>>> = Mutex::new(RefCell::new(None));
static WATCH_FACE_DIRTY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static LAST_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

// uses a simple stack for navigation history
fn nav_push(p: Page) {
    critical_section::with(|cs| {
        NAV_HISTORY.borrow(cs).borrow_mut().push(p);
    });
}
fn nav_pop() -> Option<Page> {
    critical_section::with(|cs| NAV_HISTORY.borrow(cs).borrow_mut().pop())
}

// UI State representation
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UiState {
    pub page: Page,
    pub dialog: Option<Dialog>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ClockEditState {
    digits: [u8; 4], // HHMM digits
    idx: u8,         // active digit 0-3
}

#[derive(Copy, Clone, Default)]
struct HandCache {
    sec: Option<Point>,
    min: Option<Point>,
    hour: Option<Point>,
}

impl HandCache {
    const fn new() -> Self {
        Self {
            sec: None,
            min: None,
            hour: None,
        }
    }
}

// Different pages in the UI
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Main(MainMenuState),
    Watch(WatchAppState),
    Settings(SettingsMenuState),
    Omnitrix(OmnitrixState),
    EasterEgg,
}

// Dialogs that can overlay on top of pages
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    TransformPage,
}

// States for Main Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainMenuState {
    Home,        // just show home
    WatchApp,    // enter watch app (analog/digital)
    SettingsApp, // enter Settings
    EasterEggApp,     // enter Easter Egg
}

// States for Watch App
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WatchAppState {
    Analog,
    Digital,
}

// Simple software clock: base seconds and ticks when set.
static CLOCK_BASE_SECS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));
static CLOCK_BASE_TICKS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));

pub fn set_clock_seconds(seconds: u32) {
    // Set the software clock to the specified seconds since epoch
    let now = SystemTimer::unit_value(Unit::Unit0);
    critical_section::with(|cs| {
        *CLOCK_BASE_SECS.borrow(cs).borrow_mut() = seconds as u64;
        *CLOCK_BASE_TICKS.borrow(cs).borrow_mut() = now;
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
        *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
    });
}

pub fn watch_edit_active() -> bool {
    // Check if clock edit mode is active
    critical_section::with(|cs| CLOCK_EDIT.borrow(cs).borrow().is_some())
}

pub fn watch_edit_start() {
    // Initialize edit state with current time
    let now = clock_now_seconds();
    let total_mins = now / 60;
    let h = ((total_mins / 60) % 24) as u8;
    let m = (total_mins % 60) as u8;
    let digits = [
        h / 10,
        h % 10,
        m / 10,
        m % 10,
    ];

    // Set edit state
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = Some(ClockEditState { digits, idx: 0 });
    });
}

pub fn watch_edit_cancel() {
    // Clear edit state without committing changes
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = None;
    });
}

pub fn watch_edit_advance() {
    // Move to next digit or commit changes if on last digit
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        if let Some(mut ed) = *guard {
            if ed.idx < 3 {
                ed.idx += 1;
                *guard = Some(ed);
            } else {
                // Commit
                let hours = (ed.digits[0] as u32) * 10 + (ed.digits[1] as u32);
                let mins = (ed.digits[2] as u32) * 10 + (ed.digits[3] as u32);
                let secs = (hours * 60 + mins) * 60;
                set_clock_seconds(secs);
                *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
                *guard = None;
            }
        }
    });
}

pub fn watch_edit_adjust(delta: i32) {
    // Adjust the active digit by delta (+1 or -1)
    if delta == 0 {
        return;
    }
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        // Adjust active digit
        if let Some(mut ed) = *guard {
            let idx = ed.idx as usize;
            let mut digit = ed.digits[idx] as i32;
            // Determine min/max for digit
            let (min_d, max_d) = match idx {
                0 => (0, 2),
                1 => {
                    if ed.digits[0] == 2 { (0, 3) } else { (0, 9) }
                }
                2 => (0, 5),
                _ => (0, 9),
            };
            // Adjust digit
            digit += delta;
            // Wrap around
            if digit > max_d { digit = min_d; }
            if digit < min_d { digit = max_d; }

            // Update digit
            ed.digits[idx] = digit as u8;
            *guard = Some(ed);
        }
    });
}


fn clock_now_seconds() -> u64 {
    // Get current software clock time in seconds since epoch
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second();
        let elapsed = now.saturating_sub(base_ticks) / tps;
        base_secs.saturating_add(elapsed)
    })
}

fn clock_now_seconds_f32() -> f32 {
    // Get current software clock time in seconds since epoch as f32
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow() as f32;
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second() as f32;
        let elapsed = (now.saturating_sub(base_ticks)) as f32 / tps;
        base_secs + elapsed
    })
}

// States for Settings Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsMenuState {
    Volume,
    Brightness,
    Reset,
}

// States for Omnitrix Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OmnitrixState {
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
}

impl UiState {
    // Move to the next item/state in the current layer (rotary CW)
    pub fn next_item(self) -> Self {
        if self.dialog.is_some() {
            return self;
        }
        let next_page = match self.page {
            Page::Main(state) => {
                let next = match state {
                    MainMenuState::Home => MainMenuState::WatchApp,
                    MainMenuState::WatchApp => MainMenuState::SettingsApp,
                    MainMenuState::SettingsApp => MainMenuState::EasterEggApp,
                    MainMenuState::EasterEggApp => MainMenuState::Home,
                };
                Page::Main(next)
            }
            Page::Watch(state) => {
                let next = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(next)
            }
            Page::Settings(state) => {
                let next = match state {
                    SettingsMenuState::Volume => SettingsMenuState::Brightness,
                    SettingsMenuState::Brightness => SettingsMenuState::Reset,
                    SettingsMenuState::Reset => SettingsMenuState::Volume,
                };
                Page::Settings(next)
            }
            Page::Omnitrix(state) => {
                let next = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien2,
                    OmnitrixState::Alien2 => OmnitrixState::Alien3,
                    OmnitrixState::Alien3 => OmnitrixState::Alien4,
                    OmnitrixState::Alien4 => OmnitrixState::Alien5,
                    OmnitrixState::Alien5 => OmnitrixState::Alien6,
                    OmnitrixState::Alien6 => OmnitrixState::Alien7,
                    OmnitrixState::Alien7 => OmnitrixState::Alien8,
                    OmnitrixState::Alien8 => OmnitrixState::Alien9,
                    OmnitrixState::Alien9 => OmnitrixState::Alien10,
                    OmnitrixState::Alien10 => OmnitrixState::Alien1,
                };
                Page::Omnitrix(next)
            }
            Page::EasterEgg => Page::EasterEgg,
        };
        Self {
            page: next_page,
            dialog: None,
        }
    }

    // Move to the previous item/state (rotary CCW)
    pub fn prev_item(self) -> Self {
        if self.dialog.is_some() {
            return self;
        }
        let prev_page = match self.page {
            Page::Main(state) => {
                let prev = match state {
                    MainMenuState::Home => MainMenuState::EasterEggApp,
                    MainMenuState::WatchApp => MainMenuState::Home,
                    MainMenuState::SettingsApp => MainMenuState::WatchApp,
                    MainMenuState::EasterEggApp => MainMenuState::SettingsApp,
                };
                Page::Main(prev)
            }
            Page::Watch(state) => {
                let prev = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(prev)
            }
            Page::Settings(state) => {
                let prev = match state {
                    SettingsMenuState::Volume => SettingsMenuState::Reset,
                    SettingsMenuState::Brightness => SettingsMenuState::Volume,
                    SettingsMenuState::Reset => SettingsMenuState::Brightness,
                };
                Page::Settings(prev)
            }
            Page::Omnitrix(state) => {
                let prev = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien10,
                    OmnitrixState::Alien2 => OmnitrixState::Alien1,
                    OmnitrixState::Alien3 => OmnitrixState::Alien2,
                    OmnitrixState::Alien4 => OmnitrixState::Alien3,
                    OmnitrixState::Alien5 => OmnitrixState::Alien4,
                    OmnitrixState::Alien6 => OmnitrixState::Alien5,
                    OmnitrixState::Alien7 => OmnitrixState::Alien6,
                    OmnitrixState::Alien8 => OmnitrixState::Alien7,
                    OmnitrixState::Alien9 => OmnitrixState::Alien8,
                    OmnitrixState::Alien10 => OmnitrixState::Alien9,
                };
                Page::Omnitrix(prev)
            }
            Page::EasterEgg => Page::EasterEgg,
        };
        Self {
            page: prev_page,
            dialog: None,
        }
    }

    // Go back (Button 1)
    pub fn back(self) -> Self {
        if self.dialog.is_some() {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        if let Some(prev) = nav_pop() {
            return Self {
                page: prev,
                dialog: None,
            };
        }
        // Fallback if no history
        Self {
            page: Page::Main(MainMenuState::Home),
            dialog: None,
        }
    }

    // Select/enter (Button 2)
    pub fn select(self) -> Self {
        if let Some(_) = self.dialog {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        match self.page {
        Page::Main(state) => {
            nav_push(Page::Main(state));
            let page = match state {
                MainMenuState::Home => Page::Omnitrix(OmnitrixState::Alien1),
                MainMenuState::WatchApp => Page::Watch(WatchAppState::Analog),
                MainMenuState::SettingsApp => Page::Settings(SettingsMenuState::Volume),
                MainMenuState::EasterEggApp => Page::EasterEgg,
            };
            Self { page, dialog: None }
        }
            Page::Watch(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Settings(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Omnitrix(_) => Self {
                page: self.page,
                dialog: None,
            }, // changed
            Page::EasterEgg => Self {
                page: self.page,
                dialog: None,
            },
        }
    }

    // Omnitrix transform (Button 3)
    pub fn transform(self) -> Self {
        // Only if on Omnitrix and no dialog already
        if matches!(self.page, Page::Omnitrix(_)) && self.dialog.is_none() {
            Self {
                page: self.page,
                dialog: Some(Dialog::TransformPage),
            }
        } else {
            self
        }
    }
}

// helper function to draw centered text
fn draw_text(
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
        // Prefer no-FB clear if available and requested
        if !update_fb {
            if let Some(co) =
                (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
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

// Format current clock as HH:MM into the provided 5-byte buffer and return it as &str.
fn format_clock_hm(buf: &mut [u8; 5]) -> &str {
    let total_secs = clock_now_seconds();
    let total_mins = total_secs / 60;
    let h = (total_mins / 60) % 24;
    let m = total_mins % 60;

    buf[0] = b'0' + (h / 10) as u8;
    buf[1] = b'0' + (h % 10) as u8;
    buf[2] = b':';
    buf[3] = b'0' + (m / 10) as u8;
    buf[4] = b'0' + (m % 10) as u8;

    core::str::from_utf8(buf).unwrap_or("??:??")
}

fn rgb565_from_888(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new((r >> 3) as u8, (g >> 2) as u8, (b >> 3) as u8)
}

fn hand_end(cx: i32, cy: i32, angle_deg: f32, length: i32) -> Point {
    let ang = angle_deg.to_radians();
    let dx = (cosf(ang) * length as f32) as i32;
    let dy = (sinf(ang) * length as f32) as i32;
    Point::new(cx + dx, cy + dy)
}

fn draw_hand_line(
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

fn draw_analog_clock(disp: &mut impl PanelRgb565) {
    let center = (RESOLUTION as i32 / 2, RESOLUTION as i32 / 2);
    let cx = center.0;
    let cy = center.1;

    // Current time in fractional hours, minutes, seconds
    let total_secs_f = clock_now_seconds_f32(); // necessary for smooth second hand
    let s = total_secs_f % 60.0;
    let m_total = total_secs_f / 60.0;
    let m = (m_total % 60.0) + s / 60.0;
    let h_total = m_total / 60.0;
    let h = (h_total % 12.0) + m / 60.0;

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
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let (bbox, _) = critical_section::with(|cs| {
            let mut cache = HAND_CACHE.borrow(cs).borrow_mut();
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
            if let Some(p) = cache.sec { add_pt(p, sec_pad); }
            if let Some(p) = cache.min { add_pt(p, min_pad); }
            if let Some(p) = cache.hour { add_pt(p, hour_pad); }

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
            co.draw_line_fb(cx, cy, hour_end.x, hour_end.y, Rgb565::WHITE, hour_stroke as u8);
            // Minute hand
            co.draw_line_fb(cx, cy, min_end.x, min_end.y, Rgb565::YELLOW, min_stroke as u8);
            // Second hand
            co.draw_line_fb(cx, cy, sec_end.x, sec_end.y, Rgb565::CYAN, sec_stroke as u8);
            // Center dot as solid circle
            let r_outer: i32  = 8;
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
                    if d2 > r_outer2 { continue; }
                    co.fill_rect_fb(xx, yy, xx, yy, c_solid);
                }
            }

            cache.sec = Some(sec_end);
            cache.min = Some(min_end);
            cache.hour = Some(hour_end);
            (
                (
                    minx.clamp(0, (RESOLUTION - 1) as i32),
                    miny.clamp(0, (RESOLUTION - 1) as i32),
                    maxx.clamp(0, (RESOLUTION - 1) as i32),
                    maxy.clamp(0, (RESOLUTION - 1) as i32),
                ),
                (),
            )
        });

        let (minx, miny, maxx, maxy) = bbox;
        let _ = co.flush_rect_even(minx as u16, miny as u16, maxx as u16, maxy as u16);
        return;
    }

    // Fallback: use embedded-graphics path (may flicker more).
    draw_hand_line(disp, cx, cy, sec_end, Rgb565::RED, 2);
    draw_hand_line(disp, cx, cy, min_end, Rgb565::GREEN, 3);
    draw_hand_line(disp, cx, cy, hour_end, Rgb565::BLUE, 4);
}

fn draw_transform_overlay(disp: &mut impl PanelRgb565) {
    // Simple lime DNA-like helix animation for the transform dialog.
    let t = clock_now_seconds_f32() * 2.0; // speed multiplier
    let amp_max = (RESOLUTION as f32) * 0.26;
    let step = 18;
    let cx = CENTER;
    let y_start = 12;
    let y_end = RESOLUTION as i32 - 12;
    // Use front/back shades to sell spin (swap per phase).
    let strand_a_front = rgb565_from_888(0xB8, 0xFF, 0x6A);
    let strand_a_back = rgb565_from_888(0x5F, 0xC5, 0x1C);
    let strand_b_front = rgb565_from_888(0x9F, 0xFF, 0x4A);
    let strand_b_back = rgb565_from_888(0x56, 0xB8, 0x12);
    let rung_front = rgb565_from_888(0xA8, 0xFF, 0x5A);
    let rung_back = rgb565_from_888(0x6C, 0xCC, 0x24);
    let strand_thick = 7u8;
    let rung_thick = 3u8;

    // Bounding box for the helix drawing (reuse for clear/flush).
    let pad = (amp_max as i32 + 20).min(CENTER);
    let x0 = (cx - pad).clamp(0, (RESOLUTION - 1) as i32);
    let x1 = (cx + pad).clamp(0, (RESOLUTION - 1) as i32);
    let y0 = (y_start - 8).clamp(0, (RESOLUTION - 1) as i32);
    let y1 = (y_end + 8).clamp(0, (RESOLUTION - 1) as i32);

    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        // Clear only the helix region in the framebuffer each frame.
        co.fill_rect_fb(x0, y0, x1, y1, Rgb565::BLACK);

        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;

        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            let phase = t + (i as f32) * 0.35;
            // Uniform amplitude for cylindrical look.
            let amp = amp_max * 0.75;
            let off = (sinf(phase) * amp) as i32;
            let xa = cx + off;
            let xb = cx - off;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);
            let front_side = sinf(phase) >= 0.0;
            let col_a = if front_side { strand_a_front } else { strand_a_back };
            let col_b = if front_side { strand_b_back } else { strand_b_front };
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
                // Hint depth with a darker trail
                let _ = co.draw_line_fb(p.x, p.y, pa.x, pa.y, col_a_sh, strand_thick);
                let _ = co.draw_line_fb(p.x, p.y, pa.x, pa.y, col_a, strand_thick.saturating_sub(2));
            }
            if let Some(p) = prev_b {
                let _ = co.draw_line_fb(p.x, p.y, pb.x, pb.y, col_b_sh, strand_thick);
                let _ = co.draw_line_fb(p.x, p.y, pb.x, pb.y, col_b, strand_thick.saturating_sub(2));
            }

            // Curved rung: bend slightly using a midpoint offset for a faux spin effect.
            let mid_phase = phase + core::f32::consts::FRAC_PI_2;
            let mid_bend = (sinf(mid_phase) * amp * 0.18) as i32;
            let mid_x = cx + mid_bend;
            let mid_y = y + step as i32 / 2;
            let pm = Point::new(mid_x, mid_y);
            let col_rung = if front_side { rung_front } else { rung_back };

            let _ = co.draw_line_fb(pa.x, pa.y, pm.x, pm.y, col_rung, rung_thick);
            let _ = co.draw_line_fb(pm.x, pm.y, pb.x, pb.y, col_rung, rung_thick);

            prev_a = Some(pa);
            prev_b = Some(pb);
        }

        // Flush only the helix region to avoid needless panel churn.
        let _ = co.flush_rect_even(x0 as u16, y0 as u16, x1 as u16, y1 as u16);
    } else {
        // Fallback path using embedded-graphics primitives.
        let _ = Rectangle::new(Point::new(x0, y0), Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(disp);
        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;
        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            let phase = t + (i as f32) * 0.35;
            let amp = amp_max * 0.75;
            let off = (sinf(phase) * amp) as i32;
            let xa = cx + off;
            let xb = cx - off;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);
            let front_side = sinf(phase) >= 0.0;
            let col_a = if front_side { strand_a_front } else { strand_a_back };
            let col_b = if front_side { strand_b_back } else { strand_b_front };
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

            if let Some(p) = prev_a {
                let _ = Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(col_a_sh, strand_thick.into()))
                    .draw(disp);
                let _ = Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_a,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }
            if let Some(p) = prev_b {
                let _ = Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(col_b_sh, strand_thick.into()))
                    .draw(disp);
                let _ = Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_b,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }
            let mid_phase = phase + core::f32::consts::FRAC_PI_2;
            let mid_bend = (sinf(mid_phase) * amp * 0.18) as i32;
            let mid_x = cx + mid_bend;
            let mid_y = y + step as i32 / 2;
            let pm = Point::new(mid_x, mid_y);
            let col_rung = if front_side { rung_front } else { rung_back };

            let _ = Line::new(pa, pm)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);
            let _ = Line::new(pm, pb)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);

            prev_a = Some(pa);
            prev_b = Some(pb);
        }
    }
}

fn draw_clock_edit(disp: &mut impl PanelRgb565, ed: ClockEditState) {
    // Build HH:MM string from digits
    let mut buf = [b'0'; 5];
    buf[0] = b'0' + ed.digits[0];
    buf[1] = b'0' + ed.digits[1];
    buf[2] = b':';
    buf[3] = b'0' + ed.digits[2];
    buf[4] = b'0' + ed.digits[3];
    let msg = core::str::from_utf8(&buf).unwrap_or("00:00");

    let font = &FONT_10X20; // largest built-in mono ASCII font available

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
    let rect = Rectangle::new(
        Point::new(underline_x, base_y),
        Size::new(char_w as u32, 2),
    );
    rect.into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN))
        .draw(disp)
        .ok();
}

fn ensure_watch_background_loaded() -> bool {
    critical_section::with(|cs| {
        if WATCH_BG.borrow(cs).borrow().is_some() {
            return true;
        }
        if let Ok(decompressed) = decompress_to_vec_zlib_with_limit(
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
            if let Some(co) =
                (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
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
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
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

// Map asset id to cache slot index, dimensions, and compressed blob
fn asset_meta(id: AssetId) -> (usize, u32, u32, &'static [u8]) {
    match id {
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
    }
}

fn asset_id_for_state(s: OmnitrixState) -> AssetId {
    match s {
        OmnitrixState::Alien1 => AssetId::Alien1,
        OmnitrixState::Alien2 => AssetId::Alien2,
        OmnitrixState::Alien3 => AssetId::Alien3,
        OmnitrixState::Alien4 => AssetId::Alien4,
        OmnitrixState::Alien5 => AssetId::Alien5,
        OmnitrixState::Alien6 => AssetId::Alien6,
        OmnitrixState::Alien7 => AssetId::Alien7,
        OmnitrixState::Alien8 => AssetId::Alien8,
        OmnitrixState::Alien9 => AssetId::Alien9,
        OmnitrixState::Alien10 => AssetId::Alien10,
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
            if tmp.len() == need {
                let leaked: &'static mut [u8] = alloc::boxed::Box::leak(tmp.into_boxed_slice());
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
        AssetId::InfoPage,
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

// helper function to update the display based on UI_STATE
pub fn update_ui(disp: &mut impl PanelRgb565, state: UiState, redraw: bool) {
    // If caller does not want a redraw this cycle, bail out early.
    if !redraw {
        return;
    }
    // Clear when:
    // - entering Omnitrix from another page, OR
    // - exiting Transform dialog while staying in Omnitrix
    let current_kind = match state.page {
        Page::Main(_) => PageKind::Main,
        Page::Settings(_) => PageKind::Settings,
        Page::Omnitrix(_) => PageKind::Omnitrix,
        Page::EasterEgg => PageKind::EasterEgg,
        Page::Watch(_) => PageKind::Watch,
    };
    let current_transform_active = matches!(state.page, Page::Omnitrix(_))
        && matches!(state.dialog, Some(Dialog::TransformPage));

    let should_clear_no_fb = critical_section::with(|cs| {
        let mut last_kind = LAST_PAGE_KIND.borrow(cs).borrow_mut();
        let mut last_tx = LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();

        let entering_omni =
            current_kind == PageKind::Omnitrix && *last_kind != Some(PageKind::Omnitrix);
        let exiting_transform =
            (*last_tx) && current_kind == PageKind::Omnitrix && !current_transform_active;

        // update trackers for next frame
        *last_kind = Some(current_kind);
        *last_tx = current_transform_active;

        entering_omni || exiting_transform
    });

    if should_clear_no_fb {
        let _ = if let Some(co) =
            (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
        {
            co.fill_rect_solid_no_fb(0, 0, RESOLUTION as u16, RESOLUTION as u16, Rgb565::BLACK)
                .ok();
        } else {
            disp.clear(Rgb565::BLACK).ok();
        };
    }

    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::TransformPage => {
                // On first entry into Transform dialog, hard clear the whole screen.
                let entering = critical_section::with(|cs| {
                    let mut last = LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();
                    let was = *last;
                    *last = true;
                    !was
                });
                if entering {
                    if let Some(co) =
                        (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
                    {
                        let _ = co.fill_rect_solid_no_fb(
                            0,
                            0,
                            RESOLUTION as u16,
                            RESOLUTION as u16,
                            Rgb565::BLACK,
                        );
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
        }
        return;
    }

    // Reset watch-state tracker if we’re not on the Watch page.
    if !matches!(state.page, Page::Watch(_)) {
        critical_section::with(|cs| {
            *LAST_WATCH_STATE.borrow(cs).borrow_mut() = None;
            *WATCH_BG.borrow(cs).borrow_mut() = None; // free background when leaving watch page
            *LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut() = false;
        });
    }
    // Reset transform tracker when dialog is not active.
    critical_section::with(|cs| {
        *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
    });

    match state.page {
        Page::Main(menu_state) => {
            match menu_state {
                MainMenuState::Home => {
                    // Draw the cached Omnitrix logo asset (no FB mirror)
                    if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                        draw_image_bytes(disp, buf, w, h, false, false);
                    } else if precache_asset(AssetId::Logo) {
                        if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                            draw_image_bytes(disp, buf, w, h, false, false);
                        }
                    }
                }
                MainMenuState::WatchApp => {
                    draw_text(
                        disp,
                        "Watch App",
                        Rgb565::WHITE,
                        Some(Rgb565::BLACK),
                        CENTER,
                        CENTER,
                        true,
                        true,
                        None,
                    );
                }
                MainMenuState::SettingsApp => {
                    draw_text(
                        disp,
                        "Settings (WIP)",
                        Rgb565::WHITE,
                        Some(Rgb565::BLUE),
                        CENTER,
                        CENTER,
                        true,
                        true,
                        None,
                    );
                }
                MainMenuState::EasterEggApp => {
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

        Page::Settings(settings_state) => {
            let (msg, fg, bg) = match settings_state {
                SettingsMenuState::Volume => {
                    ("Settings: Volume", Rgb565::YELLOW, Some(Rgb565::BLUE))
                }
                SettingsMenuState::Brightness => {
                    ("Settings: Brightness", Rgb565::YELLOW, Some(Rgb565::BLUE))
                }
                SettingsMenuState::Reset => ("Settings: Reset", Rgb565::YELLOW, Some(Rgb565::BLUE)),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true, true, None);
        }

        Page::Watch(watch_state) => {
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
                critical_section::with(|cs| {
                    *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                });
            }

            // If time was changed, repaint face and reset cache.
            let face_dirty = critical_section::with(|cs| {
                let mut f = WATCH_FACE_DIRTY.borrow(cs).borrow_mut();
                let dirty = *f;
                if dirty {
                    *f = false;
                }
                dirty
            });

            // If dirty, reload background and reset hand cache.
            if face_dirty {
                if ensure_watch_background_loaded() {
                    critical_section::with(|cs| {
                        if let Some(bg) = WATCH_BG.borrow(cs).borrow().as_ref() {
                            draw_image_bytes(disp, bg, RESOLUTION, RESOLUTION, false, true);
                        }
                    });
                }
                critical_section::with(|cs| {
                    *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                });
            }

            match watch_state {
                WatchAppState::Analog => {
                    draw_analog_clock(disp);
                }
                WatchAppState::Digital => {
                    // Draw either time or edit state
                    let edit = critical_section::with(|cs| *CLOCK_EDIT.borrow(cs).borrow());
                    let should_clear_after_edit = critical_section::with(|cs| {
                        let mut last = LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut();
                        let was = *last;
                        let now = edit.is_some();
                        *last = now;
                        was && !now
                    });

                    // If we were in edit mode last frame but not now, need to clear to bg
                    if should_clear_after_edit {
                        if ensure_watch_background_loaded() {
                            if let Some(bg) = critical_section::with(|cs| WATCH_BG.borrow(cs).borrow().as_ref().cloned()) {
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

        // one layer below main menu home is Omnitrix page
        Page::Omnitrix(omnitrix_state) => {
            // Note that we do not clear here, but before entering a clear happens, it is handled above for efficiency
            // Clear is necessary as the alien images don't cover the full screen
            let aid = asset_id_for_state(omnitrix_state);
            if let Some((bytes, w, h)) = get_cached_asset(aid) {
                draw_image_bytes(disp, bytes, w, h, false, false);
                // esp_println::println!("Omnitrix: drew cached image");
            } else if precache_asset(aid) {
                if let Some((bytes, w, h)) = get_cached_asset(aid) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }

        Page::EasterEgg => {
            // Draw info page image; fallback to text if not cached
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::InfoPage) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else if precache_asset(AssetId::InfoPage) {
                if let Some((bytes, w, h)) = get_cached_asset(AssetId::InfoPage) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            } else {
                disp.clear(Rgb565::WHITE).ok();
                draw_text(
                    disp,
                    "Info Screen",
                    Rgb565::CYAN,
                    None,
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
