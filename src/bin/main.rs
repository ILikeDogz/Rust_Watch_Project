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

// Add these near your other statics
static ENC_CLK: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static ENC_DT:  Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static POSITION:    Mutex<Cell<i32>> = Mutex::new(Cell::new(0));
static LAST_QSTATE: Mutex<Cell<u8>>  = Mutex::new(Cell::new(0)); // bits: [CLK<<1 | DT]
static LAST_STEP: Mutex<Cell<i8>> = Mutex::new(Cell::new(0)); // +1 or -1 from last transition

static ENC_ISR_HITS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
// System timer for timestamps
const DEBOUNCE_MS: u64 = 120;


#[inline(always)]
fn handle_button(now_ms: u64) {
    critical_section::with(|cs| {
        let mut binding = BUTTON.borrow_ref_mut(cs);
        let btn = binding.as_mut().unwrap();
        if !btn.is_interrupt_set() { return; }
        btn.clear_interrupt();

        let level_is_low = btn.is_low();
        let last_high = LAST_LEVEL.borrow(cs).get();
        LAST_LEVEL.borrow(cs).set(!level_is_low);

        if last_high && level_is_low {
            let last = LAST_INTERRUPT.borrow(cs).get();
            if now_ms.saturating_sub(last) > DEBOUNCE_MS {
                LAST_INTERRUPT.borrow(cs).set(now_ms);
                if let Some(led) = LED.borrow_ref_mut(cs).as_mut() { led.toggle(); }
            }
        }
    });
}

#[inline(always)]
fn handle_encoder() {
    // increment hit counter (use the module fn, not a method)
    critical_section::with(|cs| {
        let n = ENC_ISR_HITS.borrow(cs).get();
        ENC_ISR_HITS.borrow(cs).set(n.wrapping_add(1));
    });

    critical_section::with(|cs| {
        let mut clk_binding = ENC_CLK.borrow_ref_mut(cs);
        let mut dt_binding  = ENC_DT.borrow_ref_mut(cs);
        let clk = clk_binding.as_mut().unwrap();
        let dt  = dt_binding.as_mut().unwrap();

        let clk_pending = clk.is_interrupt_set();
        let dt_pending  = dt.is_interrupt_set();
        if clk_pending { clk.clear_interrupt(); }
        if dt_pending  { dt.clear_interrupt(); }

        let curr = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
        let prev = LAST_QSTATE.borrow(cs).get();

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

        let delta = TRANS[((prev << 2) | curr) as usize];
        // delta = -delta; // make your physical CW count positive

        if delta != 0 {
            let p = POSITION.borrow(cs).get().saturating_add(delta as i32);
            POSITION.borrow(cs).set(p);
            LAST_STEP.borrow(cs).set(delta);
        }
        LAST_QSTATE.borrow(cs).set(curr);
    });
}


#[handler]
#[ram]
fn handler() {
    let now_ms = {
        let t = SystemTimer::unit_value(Unit::Unit0);
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };
    handle_button(now_ms);
    handle_encoder();
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

    // Rotary encoder (no pull-ups, assumes external)
    let enc_cfg = InputConfig::default().with_pull(Pull::None);
    let mut clk = Input::new(peripherals.GPIO18, enc_cfg);
    let mut dt  = Input::new(peripherals.GPIO17, enc_cfg);


    // Fire ISR on any edge of either signal
    clk.listen(Event::AnyEdge);
    dt.listen(Event::AnyEdge);

    critical_section::with(|cs| {
        ENC_CLK.borrow_ref_mut(cs).replace(clk);
        ENC_DT.borrow_ref_mut(cs).replace(dt);

        // Read initial 2-bit state so first transition is well-defined
        let clk_high = ENC_CLK.borrow_ref_mut(cs).as_ref().unwrap().is_high();
        let dt_high  = ENC_DT.borrow_ref_mut(cs).as_ref().unwrap().is_high();
        let init = ((clk_high as u8) << 1) | (dt_high as u8);
        LAST_QSTATE.borrow(cs).set(init);
    });


    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

    // persist across iterations:
    let mut last_raw: u8 = 0xFF;
    let mut last_hits: u32 = 0;

    loop {
        // ISR hits (prints only when it changes)
        let hits = critical_section::with(|cs| ENC_ISR_HITS.borrow(cs).get());
        if hits != last_hits {
            esp_println::println!("Encoder ISR hits: {}", hits);
            last_hits = hits;
        }

        // Raw level change (good wiring sanity check)
        let raw = critical_section::with(|cs| {
            // read current levels
            let clk_hi = ENC_CLK.borrow_ref_mut(cs).as_ref().unwrap().is_high();
            let dt_hi  = ENC_DT.borrow_ref_mut(cs).as_ref().unwrap().is_high();
            ((clk_hi as u8) << 1) | (dt_hi as u8)
        });
        // Print only when it changes
        if raw != last_raw {
            esp_println::println!("RAW: CLK={}, DT={}", (raw >> 1) & 1, raw & 1);
            last_raw = raw;
        }

        // Detent-level direction print
        let pos = critical_section::with(|cs| POSITION.borrow(cs).get());
        let detent = pos / DETENT_STEPS; // use division (works well for negatives too)
        
        // Print only when it changes
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                // Calculate delta
                let delta = detent - prev;
                // Print direction and delta
                esp_println::println!(
                    "Encoder: {} | detent {} (Δ={})",
                    if delta > 0 { "CW" } else { "CCW" },
                    detent,
                    delta
                );
            }
            last_detent = Some(detent);
        }

        // (optional) small busy-wait or delay to reduce UART spam
    }


}
