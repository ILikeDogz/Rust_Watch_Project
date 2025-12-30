// Rotary encoder input handling

use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_hal::gpio::Input;

// Rotary encoder state struct
pub struct RotaryState<'a> {
    // pub pressed: Mutex<Cell<bool>>,
    pub clk: Mutex<RefCell<Option<Input<'a>>>>,
    pub dt: Mutex<RefCell<Option<Input<'a>>>>,
    pub position: Mutex<Cell<i32>>,
    pub last_qstate: Mutex<Cell<u8>>,
    pub last_step: Mutex<Cell<i8>>,
}

// Handle rotary encoder events
#[esp_hal::ram]
pub fn handle_encoder_generic(encoder: &RotaryState) {
    // Access encoder state within critical section
    critical_section::with(|cs| {
        let mut clk_binding = encoder.clk.borrow_ref_mut(cs);
        let mut dt_binding = encoder.dt.borrow_ref_mut(cs);
        let (Some(clk), Some(dt)) = (clk_binding.as_mut(), dt_binding.as_mut()) else {
            // encoder pins not yet installed, skip
            return;
        };

        // Check if interrupt is actually pending on either pin
        if !clk.is_interrupt_set() && !dt.is_interrupt_set() {
            return;
        }

        // Clear interrupt flags
        let clk_pending = clk.is_interrupt_set();
        let dt_pending = dt.is_interrupt_set();
        if clk_pending {
            clk.clear_interrupt();
        }
        if dt_pending {
            dt.clear_interrupt();
        }

        // Read current state of both pins
        let current = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
        let previous = encoder.last_qstate.borrow(cs).get();

        // Correct quadrature table for index = (prev<<2)|curr
        // curr order: 00, 01, 10, 11 ; prev blocks: 00, 01, 10, 11
        const TRANS: [i8; 16] = [
            // prev=00: 00, 01, 10, 11
            0, -1, 1, 0, // prev=01: 00, 01, 10, 11
            1, 0, 0, -1, // prev=10: 00, 01, 10, 11
            -1, 0, 0, 1, // prev=11: 00, 01, 10, 11
            0, 1, -1, 0,
        ];

        // Determine step delta from transition table
        let step_delta = TRANS[((previous << 2) | current) as usize];

        // Update position if there was a step
        if step_delta != 0 {
            let p = encoder
                .position
                .borrow(cs)
                .get()
                .saturating_add(step_delta as i32);
            encoder.position.borrow(cs).set(p);
            encoder.last_step.borrow(cs).set(step_delta);
        }
        // Save current state for next transition
        encoder.last_qstate.borrow(cs).set(current);
    });
}
