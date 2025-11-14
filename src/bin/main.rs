//! Watch Prototype
//! ========================================
//! needs to be run in WSL2 terminal
//! source ~/export-esp.sh
//! ========================================
//!
//! Toggles LCD when either button pressed.
//! It also reads a rotary encoder and toggles LCD.

//% CHIPS: esp32s3
//% FEATURES: esp-hal/unstable

#![no_std]
#![no_main]

// Define the application description, which is placed in a special section of the binary. 
// This is used by the bootloader to verify the application. 
// The macro automatically fills in the fields. 
esp_bootloader_esp_idf::esp_app_desc!();

use esp32s3_tests::display::setup_display;
use esp32s3_tests::ui::MainMenuState;
use esp32s3_tests::ui::Page;
use esp32s3_tests::ui::draw_hourglass_logo;
use esp32s3_tests::wiring::init_board_pins;
use esp32s3_tests::wiring::BoardPins;

use esp32s3_tests::input::{ButtonState, RotaryState, handle_button_generic, handle_encoder_generic};

use esp32s3_tests::ui::{UiState, update_ui, cache_hourglass_logo};


use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;

// ESP-HAL imports
use esp_hal::{
    handler,
    main,
    ram,
    Config,
    timer::systimer::{SystemTimer, Unit},
};

// Embedded-graphics
use embedded_graphics::{
    pixelcolor::Rgb565, 
    prelude::RgbColor,
    draw_target::DrawTarget, 
};


// use esp_hal::spi::FullDuplexMode;

#[cfg(feature = "devkit-esp32s3-disp128")]
#[ram]
static mut DISPLAY_BUF: [u8; 1024] = [0; 1024];

#[cfg(feature = "esp32s3-disp143Oled")]
static PRELOAD_TARGET_SLOTS: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));


extern crate alloc;
use alloc::{boxed::Box, vec};
use esp_hal::psram;

use core::sync::atomic::{AtomicBool, Ordering};
static BUTTON1_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON2_PRESSED: AtomicBool = AtomicBool::new(false);

// Shared resources for Button
static BUTTON1: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    // led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button1",
};

static BUTTON2: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    // led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button2",
};

// Shared resources for rotary encoder
static ROTARY: RotaryState<'static> = RotaryState {
    clk: Mutex::new(RefCell::new(None)),
    dt:  Mutex::new(RefCell::new(None)),
    position:    Mutex::new(Cell::new(0)),
    last_qstate: Mutex::new(Cell::new(0)), // bits: [CLK<<1 | DT]
    last_step: Mutex::new(Cell::new(0)), // +1 or -1 from last transition
};


static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState { page: Page::Main(MainMenuState::Home), dialog: None }));

// // for debug, start on Alien page
// static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState { page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien10), dialog: None }));


// Current debounce time (milliseconds)
const DEBOUNCE_MS: u64 = 240;


// Interrupt handler
#[handler]
#[ram]
fn handler() {
    let now_ms = {
        let t = SystemTimer::unit_value(Unit::Unit0);
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };
    
    // Button 1: JUST SET THE FLAG
    handle_button_generic(&BUTTON1, now_ms, DEBOUNCE_MS, || {
        BUTTON1_PRESSED.store(true, Ordering::Relaxed);
    });

    // Button 2: JUST SET THEFlag
    handle_button_generic(&BUTTON2, now_ms, DEBOUNCE_MS, || {
        BUTTON2_PRESSED.store(true, Ordering::Relaxed);
    });

    // Encoder logic is fine, it's just math
    handle_encoder_generic(&ROTARY);
}

use embedded_graphics::{
prelude::*,
primitives::{Rectangle, Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder},
mono_font::{ascii::FONT_6X10, MonoTextStyle},
text::{Text, Alignment},
};

use embedded_graphics::Pixel;

#[main]
fn main() -> ! {

    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

    // initial UI state
    let mut last_ui_state = UiState { page: Page::Main(MainMenuState::Home), dialog: None };

    // // for debug, start on Alien page
    // let mut last_ui_state = UiState { page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien10), dialog: None };

    
    
    // Initialize peripherals
    let peripherals = esp_hal::init(Config::default());


    esp_alloc::psram_allocator!(&peripherals.PSRAM, psram);

    // one call gives you IO handler + all your role pins from wiring.rs
    let (mut io, pins) = init_board_pins(peripherals);

    // Destructure pins for easier access
    let BoardPins {
        btn1, btn2,
        enc_clk, enc_dt,
        display_pins,
    } = pins;

    // Read encoder pin states BEFORE moving them
    let clk_initial = enc_clk.is_high() as u8;
    let dt_initial = enc_dt.is_high() as u8;
    let qstate_initial = (clk_initial << 1) | dt_initial;


    // Stash pins in global state
    critical_section::with(|cs| {
        
        BUTTON1.input.borrow_ref_mut(cs).replace(btn1);
        BUTTON1.last_level.borrow(cs).set(true);

        BUTTON2.input.borrow_ref_mut(cs).replace(btn2);
        BUTTON2.last_level.borrow(cs).set(true);

        ROTARY.clk.borrow_ref_mut(cs).replace(enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(enc_dt);
        ROTARY.last_qstate.borrow(cs).set(qstate_initial);
        ROTARY.position.borrow(cs).set(0);
        ROTARY.last_step.borrow(cs).set(0);
    });

    io.set_interrupt_handler(handler);

    let mut my_display = {
         #[cfg(feature = "devkit-esp32s3-disp128")]
         {
            // Safe because DISPLAY_BUF is only used here
            unsafe { setup_display(display_pins, &mut DISPLAY_BUF) }
         }

        #[cfg(feature = "esp32s3-disp143Oled")]
        {
            const W: usize = 466;
            let fb: &'static mut [u16] = Box::leak(vec![0u16; W * W].into_boxed_slice());

            setup_display(display_pins, fb)
        }
    };

    // Clear display
    // my_display.clear(Rgb565::BLACK).ok();


    // // quick tests
    // let ox = 273i32;  // odd
    // let oy = 241i32;  // odd
    // let ww = 77i32;  // odd
    // let hh = 39i32;  // odd

    // let it = (0..hh).flat_map(|dy| {
    //     (0..ww).map(move |dx| {
    //         let x = ox + dx;
    //         let y = oy + dy;
    //         let r = ((dx as u32 * 31) / (ww as u32)).min(31) as u8;
    //         let g = ((dy as u32 * 63) / (hh as u32)).min(63) as u8;
    //         let b = (31 - r) as u8;
    //         Pixel(Point::new(x, y), Rgb565::new(r, g, b))
    //     })
    // });
    // my_display.draw_iter(it).ok();

    // let ox = 200i32;
    // let oy = 60i32;
    // let w  = 40i32;
    // let h  = 40i32;

    // let it = (0..h).flat_map(|dy| {
    //     (0..w).filter_map(move |dx| {
    //         let x = ox + dx;
    //         let y = oy + dy;
    //         let on = ((dx ^ dy) & 1) == 0;
    //         if on {
    //             Some(Pixel(Point::new(x, y), Rgb565::WHITE))
    //         } else {
    //             None
    //         }
    //     })
    // });
    // my_display.draw_iter(it).ok();

    // let ox = 265i32;
    // let oy = 255i32;
    // let w  = 21i32; // odd
    // let h  = 17i32; // odd

    // let it = (0..h).flat_map(|dy| {
    //     (0..w).map(move |dx| {
    //         let x = ox + dx;
    //         let y = oy + dy;
    //         Pixel(Point::new(x, y), Rgb565::RED)
    //     })
    // });
    // my_display.draw_iter(it).ok();

    // // Top-left 9×7 at (1,1)
    // let it1 = (0..331i32).flat_map(|dy| {
    //     (0..49i32).map(move |dx| 
    //         Pixel(Point::new(1 + dx, 1 + dy), Rgb565::GREEN))
    // });
    // my_display.draw_iter(it1).ok();

    // // Bottom-right 11×5 ending at panel edge
    // let (width, height) = my_display.size();
    // let px_w = width as i32;
    // let px_h = height as i32;

    // let w = 81i32; // odd
    // let h = 81i32;  // odd
    // let x1 = px_w - 1;
    // let y1 = px_h - 1;
    // let ox = x1 - w + 1;
    // let oy = y1 - h + 1;

    // let it2 = (0..h).flat_map(|dy| {
    //     (0..w).map(move |dx| Pixel(Point::new(ox + dx, oy + dy), Rgb565::MAGENTA))
    // });
    // my_display.draw_iter(it2).ok();

    // // use embedded_graphics::pixelcolor::Rgb565;

    // // // Bulk fill stripes every 32 rows (should all appear)
    // // for y in (0..466u16).step_by(32) {
    // //     let _ = my_display.fill_rect_solid(0, y, 466, 2, Rgb565::new(0,31,0));
    // // }
    // // // Bulk fill stripes every 32 cols
    // // for x in (0..466u16).step_by(32) {
    // //     let _ = my_display.fill_rect_solid(x, 0, 2, 466, Rgb565::new(0,0,31));
    // // }

    // // Per-pixel overlay (white dots) to exercise tile path
    // for y in (0..466u16).step_by(16) {
    //     for x in (0..466u16).step_by(16) {
    //         my_display.draw_iter(core::iter::once(
    //             embedded_graphics::Pixel(
    //                 embedded_graphics::prelude::Point::new(x as i32, y as i32),
    //                 Rgb565::RED
    //             )
    //         )).ok();
    //     }
    // }

    
    // let x0 = 127i32; // can be odd or even
    // let y0 = 121i32;
    // let h: i32 = 59i32;

    // let it = (0..h).flat_map(|dy| {
    //     let y = y0 + dy;
    //     [
    //         Pixel(Point::new(x0, y),     Rgb565::RED),
    //         Pixel(Point::new(x0 + 1, y), Rgb565::YELLOW),
    //     ]
    // });
    // my_display.draw_iter(it).ok();

    // let x0 = 127i32; // can be odd or even
    // let y0 = 185i32;
    // let h: i32 = 59i32;

    // let it = (0..h).flat_map(|dy| {
    //     let y = y0 + dy;
    //     [
    //         Pixel(Point::new(x0, y),     Rgb565::RED),
    //         // Pixel(Point::new(x0 + 1, y), Rgb565::YELLOW),
    //     ]
    // });
    // my_display.draw_iter(it).ok();

    // let x0 = 127i32; // can be odd or even
    // let y0 = 251i32;
    // let h: i32 = 59i32;

    // let it = (0..h).flat_map(|dy| {
    //     let y = y0 + dy;
    //     [
    //         // Pixel(Point::new(x0, y),     Rgb565::RED),
    //         Pixel(Point::new(x0 + 1, y), Rgb565::YELLOW),
    //     ]
    // });
    // my_display.draw_iter(it).ok();

    // let it = core::iter::once(Pixel(Point::new(179, 183), Rgb565::RED));
    // my_display.draw_iter(it).ok();


    use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;
    use embedded_graphics::image::{ImageRawBE, Image};
    use embedded_graphics::prelude::*;
    use esp_hal::timer::systimer::{SystemTimer, Unit};

    
    // Helper: ticks -> microseconds
    let ticks_per_s = SystemTimer::ticks_per_second() as u64;
    let to_us = |t0: u64, t1: u64| -> u64 {
        let dt = t1.saturating_sub(t0);
        (dt.saturating_mul(1_000_000)) / ticks_per_s
    };

    let tb0 = SystemTimer::unit_value(Unit::Unit0);
    my_display.clear(Rgb565::WHITE).ok();
    let tb1 = SystemTimer::unit_value(Unit::Unit0);
    esp_println::println!("Clear: {} us", to_us(tb0, tb1));


    const W: u32 = 466;
    const H: u32 = 466;

    // --- Image 1 ---
    let t0 = SystemTimer::unit_value(Unit::Unit0);

    let z1: &[u8] = include_bytes!("../assets/alien2_466x466_rgb565_be.raw.zlib");
    let raw1 = decompress_to_vec_zlib_with_limit(z1, (W * H * 2) as usize).unwrap_or_default();

    let t1 = SystemTimer::unit_value(Unit::Unit0);

    if raw1.len() == (W * H * 2) as usize {
        // Draw (embedded-graphics will prefer fill_contiguous)
        let raw_img: ImageRawBE<Rgb565> = ImageRawBE::new(&raw1, W);
        let _ = Image::new(&raw_img, Point::new(0, 0)).draw(&mut my_display);
        // // Direct one-shot blit without building a scratch Vec or using e-g.
    }

    let t2 = SystemTimer::unit_value(Unit::Unit0);

    let decomp_us1 = to_us(t0, t1);
    let draw_us1   = to_us(t1, t2);
    let bytes1: u64 = (W as u64) * (H as u64) * 2;
    let kbps1 = (bytes1.saturating_mul(1_000_000) / draw_us1) / 1024;

    esp_println::println!(
        "Image1 (ImageRawBE) decompress: {} us, draw: {} us ({} KiB/s)",
        decomp_us1, draw_us1, kbps1
    );


    // --- Image 3: blit_rect_be_fast (alien4) ---
    let t6 = SystemTimer::unit_value(Unit::Unit0);

    let z3: &[u8] = include_bytes!("../assets/alien4_466x466_rgb565_be.raw.zlib");
    let raw3 = decompress_to_vec_zlib_with_limit(z3, (W * H * 2) as usize).unwrap_or_default();

    let t7 = SystemTimer::unit_value(Unit::Unit0);

    if raw3.len() == (W * H * 2) as usize {
        // Full-screen rect at (0,0). Change x,y,w,h to test sub-rect throughput.
        let _ = my_display.blit_rect_be_fast(0, 0, W as u16, H as u16, &raw3);
    }

    let t8 = SystemTimer::unit_value(Unit::Unit0);

    let decomp_us3 = to_us(t6, t7);
    let draw_us3   = to_us(t7, t8);
    let kbps3 = (bytes1.saturating_mul(1_000_000) / draw_us3) / 1024;

    esp_println::println!(
        "Image3 (blit_rect_be_fast) decompress: {} us, draw: {} us ({} KiB/s)",
        decomp_us3, draw_us3, kbps3
    );

    // --- Image 4: blit_full_frame_be_bounced (alien5) ---
    let t9 = SystemTimer::unit_value(Unit::Unit0);  
    let z4: &[u8] = include_bytes!("../assets/alien5_466x466_rgb565_be.raw.zlib");
    let raw4 = decompress_to_vec_zlib_with_limit(z4, (W * H * 2) as usize).unwrap_or_default();
    let t10 = SystemTimer::unit_value(Unit::Unit0);
    if raw4.len() == (W * H * 2) as usize {
        // Full-screen rect at (0,0). Change x,y,w,h to test sub-rect throughput.
        let _ = my_display.blit_full_frame_be_bounced(&raw4);
    }
    let t11 = SystemTimer::unit_value(Unit::Unit0);
    let decomp_us4 = to_us(t9, t10);
    let draw_us4   = to_us(t10, t11);
    let kbps4 = (bytes1.saturating_mul(1_000_000) / draw_us4) / 1024;
    esp_println::println!(
        "Image4 (bounced) decompress: {} us, draw: {} us ({} KiB/s)",
        decomp_us4, draw_us4, kbps4
    );

    // const OMNI_LIME: Rgb565 = Rgb565::new(0x11, 0x38, 0x01); // #8BE308


    // let tb0 = SystemTimer::unit_value(Unit::Unit0);
    // draw_hourglass_logo(&mut my_display, OMNI_LIME, Rgb565::BLACK, false);

    // let tb1: u64 = SystemTimer::unit_value(Unit::Unit0);
    // esp_println::println!("Hourglass draw: {} us", to_us(tb0, tb1));
    
    // Draw a white border rectangle and time it too
    let w: u32 = 200;
    let h: u32 = 200;
    let w2: u32 = w/2;
    let h2: u32 = h/2;
    let border_style = PrimitiveStyle::with_stroke(Rgb565::RED, 1);
    let tb0 = SystemTimer::unit_value(Unit::Unit0);

    Rectangle::new(Point::new((233 - (w/2)) as i32, (233 - (h/2)) as i32), Size::new(w, h))
        .into_styled(border_style)
        .draw(&mut my_display)
        .ok();
    let tb1 = SystemTimer::unit_value(Unit::Unit0);
    Rectangle::new(Point::new((233 - (w/2)) as i32, (233 - (h/2)) as i32), Size::new(w2, h2))
        .into_styled(border_style)
        .draw(&mut my_display)
        .ok();

    let tb2 = SystemTimer::unit_value(Unit::Unit0);

    esp_println::println!(
        "Border draw: {} us, upload: {} us",
        to_us(tb0, tb1),
        to_us(tb1, tb2)
    );

    use embedded_graphics::mono_font::ascii::FONT_10X20;
    use embedded_graphics::mono_font::MonoTextStyleBuilder;
    use embedded_graphics::text::Text;
    use embedded_graphics::text::Alignment;

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    // Circle params
    let cx: i32 = 233;
    let cy: i32 = 233;
    let radius: i32 = 233;
    let margin: i32 = 20; // inward padding from edge
    // Diagonal offset (≈ 45°) staying inside circle
    let diag: i32 = (((radius - margin) as f32) * 0.7071) as i32;

    // Cardinal + diagonal positions (all inside circle)
    let samples = [
        ("CENTER", cx, cy),
        ("TOP",    cx, cy - (radius - margin)),
        ("BOTTOM", cx, cy + (radius - margin)),
        ("LEFT",   cx - (radius - margin), cy),
        ("RIGHT",  cx + (radius - margin), cy),
        ("NW",     cx - diag, cy - diag),
        ("NE",     cx + diag, cy - diag),
        ("SW",     cx - diag, cy + diag),
        ("SE",     cx + diag, cy + diag),
    ];

    // Optional: draw circle outline for reference
    // use embedded_graphics::primitives::{Circle, PrimitiveStyle};
    // let _ = Circle::new(Point::new(cx, cy), (radius as u32) * 2)
    //     .into_styled(PrimitiveStyle::with_stroke(Rgb565::BLUE, 1))
    //     .draw(&mut my_display);

    for (label, x, y) in samples {
        // Skip if outside square bounds (defensive)
        if x < 0 || y < 0 || x >= 466 || y >= 466 { continue; }

        // Center alignment keeps label centered on (x,y)
        let t0 = SystemTimer::unit_value(Unit::Unit0);
        let _ = Text::with_alignment(label, Point::new(x, y), style, Alignment::Center)
            .draw(&mut my_display);
        let t1 = SystemTimer::unit_value(Unit::Unit0);
        esp_println::println!("Text '{}' draw: {} us @ ({},{})", label, to_us(t0, t1), x, y);
    }



    // // // Test 2x2 block write
    // let x: u16 = 200;
    // let y: u16 = 100;
    // let color_1 = Rgb565::RED;
    // let color_2 = Rgb565::BLACK;
    // let color_3 = Rgb565::BLACK;
    // let color_4 = Rgb565::BLACK;
    // my_display.write_2x2(x, y, color_1, color_2, color_3, color_4).ok();

    // #[cfg(feature = "esp32s3-disp143Oled")]
    // {
    //     use embedded_graphics::pixelcolor::Rgb565;

    //     // Make sure no alignment expansion interferes with the raw 2×2 tiles
    //     my_display.set_align_even(false);

    //     let x0: u16 = 100 & !1;
    //     let y0: u16 = 100 & !1;

    //     // Background is already black from clear(); FB contains black neighbors.

    //     // // Tile at (x0, y0): set TL red (others remain black from FB)
    //     let _ = my_display.write_logical_pixel(x0, y0, Rgb565::RED);

    //     // Tile at (x0+2, y0): set TR green
    //     let _ = my_display.write_logical_pixel(x0 + 2, y0, Rgb565::GREEN);

    //     // // Tile at (x0, y0+2): set BL blue
    //     // let _ = my_display.write_logical_pixel(x0, y0 + 1, Rgb565::BLUE);

    //     // // Tile at (x0+2, y0+2): set BR yellow
    //     // let _ = my_display.write_logical_pixel(x0 + 1, y0 + 1, Rgb565::YELLOW);
    // }


    // // 8×8 coarse blocks; adjust BLOCK for bigger/smaller squares.
    // const BLOCK: i32 = 16;

    // let (width, height) = my_display.size();
    // let (w, h) = (width as i32, height as i32);

    // let it = (0..w).flat_map(|x| {
    //     (0..h).filter_map(move |y| {
    //         let xb = (x / BLOCK) & 1;
    //         let yb = (y / BLOCK) & 1;
    //         if (xb ^ yb) == 1 {
    //             Some(Pixel(Point::new(x, y), Rgb565::WHITE))
    //         } else {
    //             None
    //         }
    //     })
    // });

    // let _ = my_display.draw_iter(it);

    // Helper: ticks -> microseconds
    // let ticks_per_s = SystemTimer::ticks_per_second() as u64;
    // let to_us = |t0: u64, t1: u64| -> u64 {
    //     let dt = t1.saturating_sub(t0);
    //     dt.saturating_mul(1_000_000) / ticks_per_s
    // };

    // my_display.clear(Rgb565::BLACK).ok();

    // #[cfg(feature = "esp32s3-disp143Oled")]
    // {
    //     // Pre-cache hourglass logo
    //     let lime = Rgb565::new(0x11, 0x38, 0x01); // #8BE308
    //     cache_hourglass_logo(lime, Rgb565::BLACK);
    // }

    // // Initial UI draw (timed)
    // {
    //     let t0 = SystemTimer::unit_value(Unit::Unit0);
    //     update_ui(&mut my_display, last_ui_state);
    //     let t1 = SystemTimer::unit_value(Unit::Unit0);
    //     esp_println::println!("Initial UI draw: {} us", to_us(t0, t1));
    // }

    // // Preload all images into cache (for 143-OLED, speeds up future access)
    // #[cfg(feature = "esp32s3-disp143Oled")]
    // {
    //     // Allocate one contiguous arena; it will back off if 10 doesn’t fit.
    //     let slots = esp32s3_tests::ui::init_image_arena(10);

    //     // Block here and fill each slot once (may take a few seconds)
    //     for idx in 0..slots {
    //         let _ = esp32s3_tests::ui::cache_slot(idx);
    //     }

    //     // Disable the background preloader
    //     critical_section::with(|cs| {
    //         PRELOAD_TARGET_SLOTS.borrow(cs).set(0);
    //     });
    // }
    
    // let demo_start_ms = {
    //     let t = SystemTimer::unit_value(Unit::Unit0);
    //     t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    // };

    // enum DemoState {
    //     Home,
    //     Omnitrix,
    //     Rotating { idx: usize, last_ms: u64 },
    //     BackToHome { last_ms: u64 },
    //     Done,
    // }

    // let mut demo_state = DemoState::Home;

    loop {
    //     let now_ms = {
    //         let t = SystemTimer::unit_value(Unit::Unit0);
    //         t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    //     };
    //     let rel = now_ms - demo_start_ms; // relative elapsed

    //     match demo_state {
    //         DemoState::Home => {
    //             if rel > 1000 {
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState { page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien1), dialog: None });
    //                 });
    //                 demo_state = DemoState::Omnitrix;
    //             }
    //         }
    //         DemoState::Omnitrix => {
    //             if rel > 1500 {
    //                 demo_state = DemoState::Rotating { idx: 1, last_ms: rel };
    //             }
    //         }
    //         DemoState::Rotating { idx, last_ms } => {
    //             // Rotate every 3000 ms (comment and code agree now)
    //             if rel > last_ms + 3000 {
    //                 let next_state = match idx {
    //                     1 => esp32s3_tests::ui::OmnitrixState::Alien2,
    //                     2 => esp32s3_tests::ui::OmnitrixState::Alien3,
    //                     3 => esp32s3_tests::ui::OmnitrixState::Alien4,
    //                     4 => esp32s3_tests::ui::OmnitrixState::Alien5,
    //                     5 => esp32s3_tests::ui::OmnitrixState::Alien6,
    //                     6 => esp32s3_tests::ui::OmnitrixState::Alien7,
    //                     7 => esp32s3_tests::ui::OmnitrixState::Alien8,
    //                     8 => esp32s3_tests::ui::OmnitrixState::Alien9,
    //                     9 => esp32s3_tests::ui::OmnitrixState::Alien10,
    //                     _ => esp32s3_tests::ui::OmnitrixState::Alien1,
    //                 };
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState { page: Page::Omnitrix(next_state), dialog: None });
    //                 });
    //                 if idx < 9 {
    //                     demo_state = DemoState::Rotating { idx: idx + 1, last_ms: rel };
    //                 } else {
    //                     demo_state = DemoState::BackToHome { last_ms: rel };
    //                 }
    //             }
    //         }
    //         DemoState::BackToHome { last_ms } => {
    //             if rel > last_ms + 1500 {
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState { page: Page::Main(MainMenuState::Home), dialog: None });
    //                 });
    //                 demo_state = DemoState::Done;
    //             }
    //         }
    //         DemoState::Done => { /* stop or loop again */ }
    //     }

    //     let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
    //     if ui_state != last_ui_state {
    //         let t0 = SystemTimer::unit_value(Unit::Unit0);
    //         update_ui(&mut my_display, ui_state);
    //         let t1 = SystemTimer::unit_value(Unit::Unit0);
    //         esp_println::println!("UI update: {} us", to_us(t0, t1));
    //         last_ui_state = ui_state;
    //     }

    //     for _ in 0..10000 { core::hint::spin_loop(); }
    }

    // // Main loop
    // loop {

    //     // Check for UI state changes
    //     let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
    //     if ui_state != last_ui_state {
    //         update_ui(&mut my_display, ui_state);
    //         // esp_println::println!("UI state changed: {:?}", ui_state);
    //         last_ui_state = ui_state;
    //     }

    //     // Button 1 handling
    //     if BUTTON1_PRESSED.swap(false, Ordering::Acquire) {
    //         // All work is now SAFE here in the main loop
    //         // esp_println::println!("Button 1 pressed!"); // Debug prints are safe here
    //         critical_section::with(|cs| {
    //             let state = UI_STATE.borrow(cs).get();
    //             let new_state = state.next_menu();
    //             UI_STATE.borrow(cs).set(new_state);
    //         });
    //     }

    //     // Button 2 handling
    //     if BUTTON2_PRESSED.swap(false, Ordering::Acquire) {
    //         // All work is now SAFE here in the main loop
    //         //  esp_println::println!("Button 2 pressed!"); // Debug prints are safe here
    //         critical_section::with(|cs| {
    //             let state = UI_STATE.borrow(cs).get();
    //             let new_state = state.select();
    //             UI_STATE.borrow(cs).set(new_state);
    //         });
    //     }

    //     // Rotary encoder handling
    //     let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
    //     let detent = pos / DETENT_STEPS; // use division (works well for negatives too)
        
    //     // If detent changed, update UI state
    //     if Some(detent) != last_detent {
    //         if let Some(prev) = last_detent {
    //             let step_delta = detent - prev;
    //             if step_delta > 0 {
    //                 // turned clockwise: go to next state
    //                 critical_section::with(|cs| {
    //                     // esp_println::println!("Rotary turned clockwise to detent {} pos {}", detent, pos);
    //                     let state = UI_STATE.borrow(cs).get();
    //                     let new_state = state.next_item();
    //                     UI_STATE.borrow(cs).set(new_state);
    //                 });
    //             } else if step_delta < 0 {
    //                 // turned counter-clockwise: go to previous state (optional)
    //                 critical_section::with(|cs| {
    //                     // esp_println::println!("Rotary turned counter-clockwise to detent {} pos {}", detent, pos);
    //                     let state = UI_STATE.borrow(cs).get();
    //                     let new_state = state.prev_item();
    //                     UI_STATE.borrow(cs).set(new_state);
    //                 });
    //             }
    //         }
    //         last_detent = Some(detent);
    //     }

    //     // Small delay to reduce CPU usage
    //     for _ in 0..10000 { core::hint::spin_loop(); }
    // }
}