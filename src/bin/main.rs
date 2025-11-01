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

#[cfg(feature = "esp32s3-disp143Oled")]

use esp32s3_tests::display::setup_display;
use esp32s3_tests::ui::MainMenuState;
use esp32s3_tests::ui::Page;
use esp32s3_tests::wiring::init_board_pins;
use esp32s3_tests::wiring::BoardPins;

use esp32s3_tests::input::{ButtonState, RotaryState, handle_button_generic, handle_encoder_generic};

use esp32s3_tests::ui::{UiState, update_ui};


use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;

// ESP-HAL imports
use esp_hal::{
    handler,
    main,
    ram,
    psram,
    Config,
    timer::systimer::{SystemTimer, Unit},
};

// Embedded-graphics
use embedded_graphics::{
    pixelcolor::Rgb565, 
    prelude::RgbColor,
    draw_target::DrawTarget, 
};


// use esp_println::println; // already part of esp-hal projects
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


#[cfg(feature = "esp32s3-disp143Oled")]
extern crate alloc;
#[cfg(feature = "esp32s3-disp143Oled")]
use alloc::{boxed::Box, vec, vec::Vec};
#[cfg(feature = "esp32s3-disp143Oled")]


static BUTTON1_FLAG: critical_section::Mutex<core::cell::Cell<bool>> =
    critical_section::Mutex::new(core::cell::Cell::new(false));
static BUTTON2_FLAG: critical_section::Mutex<core::cell::Cell<bool>> =
     critical_section::Mutex::new(core::cell::Cell::new(false));


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
        critical_section::with(|cs| {
            BUTTON1_FLAG.borrow(cs).set(true);
        });
    });

    // Button 2: JUST SET THEFlag
    handle_button_generic(&BUTTON2, now_ms, DEBOUNCE_MS, || {
        critical_section::with(|cs| {
            BUTTON2_FLAG.borrow(cs).set(true);
        });
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
    let peripherals = esp_hal::init(Config::default());

    #[cfg(feature = "esp32s3-disp143Oled")]
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

    #[cfg(feature = "devkit-esp32s3-disp128")]  
    let mut display_buf = [0u8; 1024];
        

    let mut my_display = {
        #[cfg(feature = "devkit-esp32s3-disp128")]
        {
            setup_display(display_pins, &mut display_buf)
        }

        #[cfg(feature = "esp32s3-disp143Oled")]
        {
            // CO5300: PSRAM-backed RGB565 framebuffer without unsafe
            // Ensure psram_allocator! was called before this allocation
            const W: usize = 466;
            let fb: &'static mut [u16] = Box::leak(vec![0u16; W * W].into_boxed_slice());
            setup_display(display_pins, fb)
        }
    };
    
    #[cfg(feature = "esp32s3-disp143Oled")]
    my_display.set_align_even(true);

    // // --- FIRST DRAW ----------------------------------------------------------

    my_display.clear(Rgb565::BLACK).ok();

     
    update_ui(&mut my_display, last_ui_state);

    // --- MAIN LOOP -----------------------------------------------------------
    loop {

        // Check for UI state changes
        let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
        if ui_state != last_ui_state {
            update_ui(&mut my_display, ui_state);
            esp_println::println!("UI state changed: {:?}", ui_state);
            last_ui_state = ui_state;
        }

        let b1 = critical_section::with(|cs| {
            let f = BUTTON1_FLAG.borrow(cs).get();
            if f { BUTTON1_FLAG.borrow(cs).set(false); }
            f
        });
        if b1 {
            // All work is now SAFE here in the main loop
            esp_println::println!("Button 1 pressed!"); // Debug prints are safe here
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.next_menu();
                UI_STATE.borrow(cs).set(new_state);
            });
        }

        // --- Handle Button 2 Press ---
        let b2 = critical_section::with(|cs| {
            let f = BUTTON2_FLAG.borrow(cs).get();
            if f { BUTTON2_FLAG.borrow(cs).set(false); }
            f
        });
        if b2 {
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
