// Button input handling

use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_hal::gpio::Input;

// Button state struct
pub struct ButtonState<'a> {
    // pub pressed: Mutex<Cell<bool>>,
    pub input: Mutex<RefCell<Option<Input<'a>>>>,
    pub last_level: Mutex<Cell<bool>>,
    pub last_interrupt: Mutex<Cell<u64>>,
    pub name: &'static str,
}

// Handle button press events
#[esp_hal::ram]
pub fn handle_button_generic(
    btn: &ButtonState,
    now_ms: u64,
    debounce_ms: u64,
    on_press: impl Fn(),
) {
    // Access button state within critical section
    critical_section::with(|cs| {
        let mut btn_binding = btn.input.borrow_ref_mut(cs);
        let input_opt = btn_binding.as_mut();
        let input = match input_opt {
            Some(i) => i,
            None => return, // pin not yet installed
        };
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
