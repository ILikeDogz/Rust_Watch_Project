//! GPIO interrupt
//!
//! This prints "Interrupt" when the boot button is pressed.
//! It also toggles an LED when either button is pressed.
//! It also reads a rotary encoder and prints direction and detent count.

//% CHIPS: esp32s3
//% FEATURES: esp-hal/unstable

#![no_std]
#![no_main]

// Define the application description, which is placed in a special section of the binary. 
// This is used by the bootloader to verify the application. 
// The macro automatically fills in the fields. 
esp_bootloader_esp_idf::esp_app_desc!();

use esp32s3_tests::wiring::init_board_pins;

use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_hal::{
    gpio::{Input, Output},
    handler,
    main,
    ram,
    timer::systimer::{SystemTimer, Unit},
};

// Shared resources for button handling
struct ButtonState<'a> {
    input: Mutex<RefCell<Option<Input<'a>>>>,
    led: Mutex<RefCell<Option<Output<'a>>>>,
    last_level: Mutex<Cell<bool>>,
    last_interrupt: Mutex<Cell<u64>>,
    name: &'static str,
}

struct RotaryState<'a> {
    clk: Mutex<RefCell<Option<Input<'a>>>>,
    dt:  Mutex<RefCell<Option<Input<'a>>>>,
    position:    Mutex<Cell<i32>>,
    last_qstate: Mutex<Cell<u8>>,  // bits: [CLK<<1 | DT]
    last_step: Mutex<Cell<i8>>, // +1 or -1 from last transition
}

// Shared resources for Button
static BUTTON1: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button1",
};

static BUTTON2: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    led: Mutex::new(RefCell::new(None)),
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


// System timer for timestamps
const DEBOUNCE_MS: u64 = 120;

// Handle button press events
fn handle_button_generic(btn: &ButtonState, now_ms: u64) {
    // Access button state within critical section
    critical_section::with(|cs| {
        let mut btn_binding = btn.input.borrow_ref_mut(cs);
        let input = btn_binding.as_mut().unwrap();

        // Check if interrupt is actually pending
        if !input.is_interrupt_set() { 
            return; 
        }
        input.clear_interrupt();

        // Debounce logic: check for falling edge and time since last event
        let level_is_low = input.is_low();
        let last_high = btn.last_level.borrow(cs).get();
        btn.last_level.borrow(cs).set(!level_is_low);

        if last_high && level_is_low {
            // Falling edge detected
            let last_debounce = btn.last_interrupt.borrow(cs).get();
            // Check debounce time
            if now_ms.saturating_sub(last_debounce) > DEBOUNCE_MS {
                // Valid press event
                btn.last_interrupt.borrow(cs).set(now_ms);
                // Toggle associated LED if available
                if let Some(led) = btn.led.borrow_ref_mut(cs).as_mut() { 
                    led.toggle(); 
                    esp_println::println!("{} pressed, LED is now {}", btn.name, if led.is_set_high() { "ON" } else { "OFF" });
                }
            }
        }
    });
}

#[inline(always)]
fn handle_encoder_generic(encoder: &RotaryState) {
    // Access encoder state within critical section
    critical_section::with(|cs| {
        let mut clk_binding = encoder.clk.borrow_ref_mut(cs);
        let mut dt_binding  = encoder.dt.borrow_ref_mut(cs);
        let clk = clk_binding.as_mut().unwrap();
        let dt  = dt_binding.as_mut().unwrap();

        // Check if interrupt is actually pending on either pin
        if !clk.is_interrupt_set() && !dt.is_interrupt_set() { 
            return; 
        }

        // Clear interrupt flags
        let clk_pending = clk.is_interrupt_set();
        let dt_pending  = dt.is_interrupt_set();
        if clk_pending { clk.clear_interrupt(); }
        if dt_pending  { dt.clear_interrupt(); }

        // Read current state of both pins
        let curr = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
        let prev = ROTARY.last_qstate.borrow(cs).get();

        // Correct quadrature table for index = (prev<<2)|curr
        // curr order: 00, 01, 10, 11 ; prev blocks: 00, 01, 10, 11
        const TRANS: [i8; 16] = [
            // prev=00: 00, 01, 10, 11
            0, -1, 1,  0,
            // prev=01: 00, 01, 10, 11
            1,  0,  0, -1,
            // prev=10: 00, 01, 10, 11
            -1,  0,  0, 1,
            // prev=11: 00, 01, 10, 11
            0, 1, -1,  0,
        ];

        // Determine step delta from transition table
        let delta = TRANS[((prev << 2) | curr) as usize];

        // Update position if there was a step
        if delta != 0 {
            let p = ROTARY.position.borrow(cs).get().saturating_add(delta as i32);
            ROTARY.position.borrow(cs).set(p);
            ROTARY.last_step.borrow(cs).set(delta);
        }
        // Save current state for next transition
        ROTARY.last_qstate.borrow(cs).set(curr);
    });
}



// Interrupt handler
#[handler]
#[ram]
fn handler() {
    let now_ms = {
        let t = SystemTimer::unit_value(Unit::Unit0);
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };
    handle_button_generic(&BUTTON1, now_ms);
    handle_button_generic(&BUTTON2, now_ms);
    // handle_encoder();
    handle_encoder_generic(&ROTARY);
}


#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // one call gives you IO handler + all your role pins
    let (mut io, pins) = init_board_pins(peripherals);
    io.set_interrupt_handler(handler);

    // stash the pins into your globals
    critical_section::with(|cs| {
        BUTTON1.led.borrow_ref_mut(cs).replace(pins.led1);
        BUTTON1.input.borrow_ref_mut(cs).replace(pins.btn1);
        BUTTON1.last_level.borrow(cs).set(true);

        BUTTON2.led.borrow_ref_mut(cs).replace(pins.led2);
        BUTTON2.input.borrow_ref_mut(cs).replace(pins.btn2);
        BUTTON2.last_level.borrow(cs).set(true);

        ROTARY.clk.borrow_ref_mut(cs).replace(pins.enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(pins.enc_dt);
    });


    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

    loop {

        // Detent-level direction print
        let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
        let detent = pos / DETENT_STEPS; // use division (works well for negatives too)
        
        // Print only when it changes
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                // Calculate delta
                let delta = detent - prev;
                // Print direction and delta
                esp_println::println!(
                    "Encoder: {} | detent {} (Δ={})",
                    if delta > 0 { "ClockWise" } else { "CounterClockWise" },
                    detent,
                    delta
                );
            }
            // record last detent
            last_detent = Some(detent);
        }

        // (optional) small busy-wait or delay to reduce UART spam
    }


}
