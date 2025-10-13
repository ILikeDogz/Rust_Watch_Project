//! Input handling module for buttons and rotary encoder.
//!
//! This module provides:
//! - `ButtonState` and `RotaryState` structs for tracking input state
//! - Debounced button event handling via `handle_button_generic`
//! - Rotary encoder quadrature decoding via `handle_encoder_generic`
//!
//! All input state is protected with `critical_section` for safe concurrent access in interrupt and main contexts.
//! Designed for use with ESP-HAL GPIO and embedded Rust applications.


use esp_backtrace as _;

use core::cell::{Cell, RefCell};
use critical_section::Mutex;

// ESP-HAL imports
use esp_hal::gpio::Input;

// Button state struct
pub struct ButtonState<'a> {
    pub input: Mutex<RefCell<Option<Input<'a>>>>,
    pub last_level: Mutex<Cell<bool>>,
    pub last_interrupt: Mutex<Cell<u64>>,
    pub name: &'static str,
}

// Rotary encoder state struct
pub struct RotaryState<'a> {
    pub clk: Mutex<RefCell<Option<Input<'a>>>>,
    pub dt:  Mutex<RefCell<Option<Input<'a>>>>,
    pub position:    Mutex<Cell<i32>>,
    pub last_qstate: Mutex<Cell<u8>>,
    pub last_step: Mutex<Cell<i8>>,
}

// Handle button press events
pub fn handle_button_generic(btn: &ButtonState, now_ms: u64, debounce_ms: u64, on_press: impl Fn()) {
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
            if now_ms.saturating_sub(last_debounce) > debounce_ms {
                btn.last_interrupt.borrow(cs).set(now_ms);
                on_press();
            }
        }
    });
}

#[inline(always)]
pub fn handle_encoder_generic(encoder: &RotaryState) {
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
        let current = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
        let previous = encoder.last_qstate.borrow(cs).get();

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
        let step_delta = TRANS[((previous << 2) | current) as usize];

        // Update position if there was a step
        if step_delta != 0 {
            let p = encoder.position.borrow(cs).get().saturating_add(step_delta as i32);
            encoder.position.borrow(cs).set(p);
            encoder.last_step.borrow(cs).set(step_delta);
        }
        // Save current state for next transition
        encoder.last_qstate.borrow(cs).set(current);
    });
}
