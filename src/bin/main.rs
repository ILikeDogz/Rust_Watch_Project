//! GPIO interrupt
//!
//! This prints "Interrupt" when the boot button is pressed.
//! It also blinks an LED like the blinky example.
//!
//! The following wiring is assumed:
//! - LED => GPIO2
//! - BUTTON => GPIO15

//% CHIPS: esp32 esp32c2 esp32c3 esp32c6 esp32h2 esp32s2 esp32s3
//% FEATURES: esp-hal/unstable

#![no_std]
#![no_main]

use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    handler,
    main,
    ram,
    timer::systimer::{SystemTimer, Unit},
};

// Define the application description, which is placed in a special section of the binary. 
// This is used by the bootloader to verify the application. 
// The macro automatically fills in the fields. 
esp_bootloader_esp_idf::esp_app_desc!();

// Shared resources between main and the interrupt handler
static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static LED:    Mutex<RefCell<Option<Output>>> = Mutex::new(RefCell::new(None));
static LAST_LEVEL:   Mutex<Cell<bool>> = Mutex::new(Cell::new(true)); // true = High (idle with pull-up)
static LAST_INTERRUPT: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));

// System timer for timestamps
const DEBOUNCE_MS: u64 = 120;

#[handler]
#[ram]
// Interrupt handler for the button press 
fn handler() {

    // Timestamp in ms
    let now_ms = {
        // convert ticks to ms
        let ticks = SystemTimer::unit_value(Unit::Unit0);
        ticks.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };

    // Read level + clear the interrupt flag ASAP (needs &mut)
    let level_is_low = critical_section::with(|cs| {
        // borrow mutably to read level and clear interrupt
        let mut binding = BUTTON.borrow_ref_mut(cs);
        let btn = binding.as_mut().unwrap();
        
        btn.clear_interrupt(); // always clear

        esp_println::println!("ISR: level is low: {}", btn.is_low());

        // return level
        btn.is_low()
    });

    // Edge + time-based debounce
    let should_handle = critical_section::with(|cs| {
        let last_level_was_high = LAST_LEVEL.borrow(cs).get();
        // store current level: high = true, low = false
        LAST_LEVEL.borrow(cs).set(!level_is_low);

        // Check if the button was the source of the interrupt
        esp_println::println!("ISR: Button interrupt set: {}", BUTTON.borrow_ref_mut(cs).as_mut().unwrap().is_interrupt_set());
        // Only handle High -> Low edges
        if last_level_was_high && level_is_low {
            // Check debounce
            let last_t = LAST_INTERRUPT.borrow(cs).get();
            // If more than DEBOUNCE_MS ms since last valid interrupt, accept this one
            if now_ms.saturating_sub(last_t) > DEBOUNCE_MS {
                LAST_INTERRUPT.borrow(cs).set(now_ms);
                true
            } 
            // else ignore (too soon)
            else {
                false
            }
        } 
        // Ignore Low -> High edges
        else {
            false
        }
    });

    // If we shouldn't handle this interrupt, return early
    if !should_handle {
        return;
    }

    // If made it this far valid button press detected
    critical_section::with(|cs| {
        if let Some(led) = LED.borrow_ref_mut(cs).as_mut() {
            esp_println::println!("Button pressed! Toggling LED.");
            led.toggle();
        }
    });
}


#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(handler);

    // LED
    let mut led = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    led.set_high();
    critical_section::with(|cs| {
        LED.borrow_ref_mut(cs).replace(led);
    });

    // Button (pull-up, idle High)
    let cfg = InputConfig::default().with_pull(Pull::Up);
    let mut btn = Input::new(peripherals.GPIO15, cfg);

    // Listen on both edges, but only act on High->Low in the ISR
    btn.listen(Event::AnyEdge);

    critical_section::with(|cs| {
        BUTTON.borrow_ref_mut(cs).replace(btn);
        LAST_LEVEL.borrow(cs).set(true); // start idle High
    });

    loop {}
}
