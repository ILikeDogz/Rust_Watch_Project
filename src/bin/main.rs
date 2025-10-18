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
use esp32s3_tests::ui::MainMenuState;
use esp32s3_tests::ui::Page;
use esp32s3_tests::wiring::init_board_pins;
use esp32s3_tests::wiring::BoardPins;

use esp32s3_tests::input::{ButtonState, RotaryState, handle_button_generic, handle_encoder_generic};

use esp32s3_tests::ui::{UiState, update_ui};

// use esp32s3_tests::display::setup_display;

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

fn blink_gpio_test(
    cs: &mut impl OutputPin,
    rst: &mut impl OutputPin,
    en: &mut impl OutputPin,
    delay: &mut impl DelayNs,
) {
    // 3 quick pulses on each line
    for _ in 0..3 {
        cs.set_low().ok();  delay.delay_ms(50);
        cs.set_high().ok(); delay.delay_ms(50);
    }
    for _ in 0..3 {
        rst.set_low().ok();  delay.delay_ms(50);
        rst.set_high().ok(); delay.delay_ms(50);
    }
    for _ in 0..3 {
        en.set_low().ok();  delay.delay_ms(50);
        en.set_high().ok(); delay.delay_ms(50);
    }
}


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
    handle_button_generic(&BUTTON1, now_ms, DEBOUNCE_MS, || {
        // Button 1: Switch menu
        esp_println::println!("{} pressed", BUTTON1.name);
        critical_section::with(|cs| {
            let state = UI_STATE.borrow(cs).get();
            let new_state = state.next_menu();
            UI_STATE.borrow(cs).set(new_state);
        });
    });
    handle_button_generic(&BUTTON2, now_ms, DEBOUNCE_MS, || {
        // Button 2: Select
        esp_println::println!("{} pressed", BUTTON2.name);
        critical_section::with(|cs| {
            let state = UI_STATE.borrow(cs).get();
            let new_state = state.select();
            UI_STATE.borrow(cs).set(new_state);
        });
    });

    handle_encoder_generic(&ROTARY);
}


// fn read_reg(spi: &mut impl embedded_hal::spi::SpiDevice<u8>, reg: u8) -> u8 {
//     let hdr = [0x03, 0x00, reg, 0x00]; // CO5300 read long-header
//     let mut v = [0u8];
//     spi.transaction(&mut [
//         embedded_hal::spi::Operation::Write(&hdr),
//         embedded_hal::spi::Operation::Read(&mut v),
//     ]).ok();
//     v[0]
// }

pub fn ramwr_stream<SD: embedded_hal::spi::SpiDevice<u8>>(
    spi: &mut SD,
    chunks: &[&[u8]],   // a list of pixel slices to send back-to-back
) {
    use embedded_hal::spi::Operation;
    // let hdr = [0x02, 0x2C]; // DCS RAMWR (single-line)
    let hdr = [0x02, 0x00, 0x2C, 0x00];
    // Build one transaction: header + N data chunks (CS stays asserted)
    // If you don't have heapless, just do two writes: header then a big concat buffer.
    let mut ops: heapless::Vec<Operation<'_, u8>, 64> = heapless::Vec::new();
    ops.push(Operation::Write(&hdr)).ok();
    for &c in chunks {
        ops.push(Operation::Write(c)).ok();
    }
    spi.transaction(&mut ops).ok();
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
    io.set_interrupt_handler(handler);

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


    // Pull out the OLED pins (with spi2 back in DisplayPins for symmetry)
    let DisplayPins {
        spi2,
        mut cs,
        clk,
        do0,    // MOSI
        do1,    // MISO
        do2,
        do3,
        mut rst,
        mut en,
        tp_sda: _,
        tp_scl: _,
    } = display_pins;

    // quick pin blink so you can find them on a scope/LA
    let mut delay = esp_hal::delay::Delay::new();
    // (assuming cs, rst, en are Output<'_>)
    blink_gpio_test(&mut cs, &mut rst, &mut en, &mut delay);


    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_hz(40_000_000))  // CO5300 works up to 40 MHz
        .with_mode(Mode::_0);                        // keep Mode0

    let spi = Spi::new(spi2, spi_cfg).unwrap()
            .with_sck(clk)    // GPIO 10
            .with_mosi(do0)   // GPIO 11
            .with_miso(do1);  // GPIO 12 (for reading)
        // Do NOT configure sio2 (GPIO 13) and sio3 (GPIO 14)

    println!("SPI created, wrapping ExclusiveDevice...");
    let spi_dev = ExclusiveDevice::new(spi, cs, NoDelay).unwrap();
    println!("ExclusiveDevice OK");

    let mut delay = esp_hal::delay::Delay::new();
    println!("Creating display...");
    let mut display = co5300::new_with_defaults(spi_dev, Some(rst), &mut delay)
        .expect("CO5300 init failed");
    println!("Display created.");

    match display.read_id() {
        Ok(id) => println!("Panel ID (expected bogus on MISO): 0x{:02X}", id),
        Err(e) => println!("read_id error: {:?}", e),
    }

    // // after init+delays:
    // let pwr = read_reg(&mut display.spi, 0x0A); // Get Power Mode
    // let pix = read_reg(&mut display.spi, 0x0C); // Get Pixel Format
    // let mad = read_reg(&mut display.spi, 0x36); // MADCTL (read)
    // println!("DCS: PWR=0x{:02X} PIX=0x{:02X} MAD=0x{:02X}", pwr, pix, mad);

    // 1×1 at (10,10)
    // after init()
    let cx = 233u16; let cy = 233u16;
    let w = 40u16; let h = 40u16;
    display.set_window(cx - w/2, cy - h/2, cx + w/2 - 1, cy + h/2 - 1).unwrap();
    let wb = embedded_graphics::pixelcolor::Rgb565::BLACK.into_storage().to_be_bytes();
    let mut row = [0u8; 40*2];
    for i in (0..row.len()).step_by(2) { row[i]=wb[0]; row[i+1]=wb[1]; }

    // build chunk list: all rows back-to-back
    let mut chunks: heapless::Vec<&[u8], 64> = heapless::Vec::new();
    for _ in 0..h { chunks.push(&row).ok(); }
    ramwr_stream(&mut display.spi, &chunks);





    loop {}
    // set up display
    // buffer for ram allocation
    // let mut display_buf = [0u8; 1024];
    // let mut my_display = setup_display(
    //     display_pins, &mut display_buf,
    // );

    // // --- FIRST DRAW ----------------------------------------------------------
    // my_display.clear(Rgb565::GREEN).ok();
    // // update_ui(&mut my_display, last_ui_state);

    // loop {

    //     // // Check for UI state changes
    //     // let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
    //     // if ui_state != last_ui_state {
    //     //     update_ui(&mut my_display, ui_state);
    //     //     last_ui_state = ui_state;
    //     // }
        
    //     // // Rotary encoder handling
    //     // let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
    //     // let detent = pos / DETENT_STEPS; // use division (works well for negatives too)
        
    //     // // If detent changed, update UI state
    //     // if Some(detent) != last_detent {
    //     //     if let Some(prev) = last_detent {
    //     //         let step_delta = detent - prev;
    //     //         if step_delta > 0 {
    //     //             // turned clockwise: go to next state
    //     //             critical_section::with(|cs| {
    //     //                 let state = UI_STATE.borrow(cs).get();
    //     //                 let new_state = state.next_item();
    //     //                 UI_STATE.borrow(cs).set(new_state);
    //     //             });
    //     //         } else if step_delta < 0 {
    //     //             // turned counter-clockwise: go to previous state (optional)
    //     //             critical_section::with(|cs| {
    //     //                 let state = UI_STATE.borrow(cs).get();
    //     //                 let new_state = state.prev_item();
    //     //                 UI_STATE.borrow(cs).set(new_state);
    //     //             });
    //     //         }
    //     //     }
    //     //     last_detent = Some(detent);
    //     // }

    //     // Small delay to reduce CPU usage
    //     for _ in 0..10000 { core::hint::spin_loop(); }
    // }
}
