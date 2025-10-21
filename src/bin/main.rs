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

use embedded_graphics::prelude::IntoStorage;
use esp32s3_tests::co5300;
use esp32s3_tests::display::setup_display;
use esp32s3_tests::ui::MainMenuState;
use esp32s3_tests::ui::Page;
use esp32s3_tests::wiring::init_board_pins;
use esp32s3_tests::wiring::BoardPins;

use esp32s3_tests::input::{ButtonState, RotaryState, handle_button_generic, handle_encoder_generic};

use esp32s3_tests::ui::{UiState, update_ui};


use esp32s3_tests::wiring::DisplayPins;
use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;

// ESP-HAL imports
use esp_hal::{
    handler,
    main,
    ram,
    timer::systimer::{SystemTimer, Unit},
};

// Embedded-graphics
use embedded_graphics::{
    pixelcolor::Rgb565, 
    prelude::RgbColor,
    draw_target::DrawTarget, 
};


use esp_println::println; // already part of esp-hal projects
use esp_hal::spi::master::{Spi, Config as SpiConfig};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use embedded_hal::{
    delay::DelayNs,
    digital::OutputPin,
    spi::SpiDevice,
};
// use esp_hal::spi::FullDuplexMode;

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


#[main]
fn main() -> ! {

    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

    // initial UI state
    let mut last_ui_state = UiState { page: Page::Main(MainMenuState::Home), dialog: None };

    // Initialize peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

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

    // set up display
    let mut display_buf = [0u8; 1024];
    let mut my_display = setup_display(
        display_pins, &mut display_buf,
    );

    #[cfg(feature = "esp32s3-disp143Oled")]
    {
        // if your type exposes set_align_even:
        my_display.set_align_even(false);
    }

    // --- FIRST DRAW ----------------------------------------------------------
    my_display.clear(Rgb565::BLACK).ok();
    
    // #[cfg(feature = "esp32s3-disp143Oled")]
    // {
    //     // // 1) single white pixel at (50,50)
    //     // my_display.set_window(50, 50, 50, 50).unwrap();
    //     // let pix = Rgb565::WHITE.into_storage().to_be_bytes();
    //     // my_display.write_pixels(&pix).unwrap();

    //     // 2) small 20x20 red block at (100,100) using a contiguous buffer
    //     const BW: usize = 300;
    //     const BH: usize = 300;
    //     let bx: u16 = 0;
    //     let by: u16 = 0;
    //     my_display.set_window(bx, by, bx + (BW as u16) - 1, by + (BH as u16) - 1).unwrap();

    //     let mut buf = [0u8; BW * BH * 2];
    //     let color = Rgb565::WHITE.into_storage().to_be_bytes();
    //     for i in (0..buf.len()).step_by(2) {
    //         buf[i] = color[0];
    //         buf[i + 1] = color[1];
    //     }
    //     my_display.write_pixels(&buf).unwrap();

    //     // Optional: also test the rows API (no copy of whole image)
    //     /*
    //     let mut row = [0u8; BW * 2];
    //     for i in (0..row.len()).step_by(2) {
    //         row[i] = red[0];
    //         row[i + 1] = red[1];
    //     }
    //     let mut rows: heapless::Vec<&[u8], BH> = heapless::Vec::new();
    //     for _ in 0..BH { rows.push(&row).ok(); }
    //     my_display.set_window(bx, by, bx + (BW as u16) - 1, by + (BH as u16) - 1).unwrap();
    //     my_display.write_pixels_rows(&rows).unwrap();
    //     */

    //     println!("co5300 write_pixels test done");
    // }

    // my_display
    //     .fill_rect_solid(
    //         233/2,
    //         233/2,
    //         233,
    //         233,
    //         Rgb565::WHITE,
    //     )
    //     .ok();

    // use embedded_graphics::{primitives::{Rectangle, PrimitiveStyle}, prelude::*};

    // // After clear()
    // Rectangle::new(Point::new(10, 10), Size::new(80, 20))
    //     .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
    //     .draw(&mut my_display)
    //     .ok();

    // // 1px line (catches single-row issues)
    // Rectangle::new(Point::new(10, 40), Size::new(120, 1))
    //     .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
    //     .draw(&mut my_display)
    //     .ok();

    // use embedded_graphics::{
    // mono_font::{ascii::{FONT_10X20, FONT_6X10}, 
    // MonoTextStyle, MonoTextStyleBuilder}, 
    // pixelcolor::Rgb565, 
    // prelude::{Point, Primitive, RgbColor, Size, OriginDimensions}, 
    // primitives::{PrimitiveStyle, Rectangle, Circle, Triangle}, text::{Alignment, Baseline, Text}, 
    // Drawable,
    // draw_target::DrawTarget, 
    // };

    // let style = MonoTextStyleBuilder::new()
    //     .font(&FONT_10X20)
    //     .text_color(Rgb565::WHITE)
    //     .background_color(Rgb565::BLACK)
    //     .build();

    // Text::with_alignment(
    //     "Watch Prototype",
    //     Point::new(0, 0),
    //     style,
    //     Alignment::Center,
    // )
    // .draw(&mut my_display)
    // .ok();
     
    // update_ui(&mut my_display, last_ui_state);

    // --- MAIN LOOP -----------------------------------------------------------
    loop {

        // Check for UI state changes
        let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
        if ui_state != last_ui_state {
            // update_ui(&mut my_display, ui_state);
            esp_println::println!("UI state changed: {:?}", ui_state);
            last_ui_state = ui_state;
        }
        
        if BUTTON1_PRESSED.swap(false, Ordering::Acquire) {
            // All work is now SAFE here in the main loop
            esp_println::println!("Button 1 pressed!"); // Debug prints are safe here
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.next_menu();
                UI_STATE.borrow(cs).set(new_state);
            });
        }

        // --- Handle Button 2 Press ---
        if BUTTON2_PRESSED.swap(false, Ordering::Acquire) {
            // All work is now SAFE here in the main loop
             esp_println::println!("Button 2 pressed!"); // Debug prints are safe here
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.select();
                UI_STATE.borrow(cs).set(new_state);
            });
        }

        // Rotary encoder handling
        let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
        let detent = pos / DETENT_STEPS; // use division (works well for negatives too)
        
        // If detent changed, update UI state
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                let step_delta = detent - prev;
                if step_delta > 0 {
                    // turned clockwise: go to next state
                    critical_section::with(|cs| {
                        esp_println::println!("Rotary turned clockwise to detent {} pos {}", detent, pos);
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.next_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                } else if step_delta < 0 {
                    // turned counter-clockwise: go to previous state (optional)
                    critical_section::with(|cs| {
                        esp_println::println!("Rotary turned counter-clockwise to detent {} pos {}", detent, pos);
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.prev_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                }
            }
            last_detent = Some(detent);
        }

        // Small delay to reduce CPU usage
        for _ in 0..10000 { core::hint::spin_loop(); }
    }
}
