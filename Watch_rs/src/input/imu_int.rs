// IMU interrupt input handling

use core::sync::atomic::AtomicBool;
use critical_section::Mutex;
use esp_hal::gpio::Input;

// Generic IMU interrupt state (active-low)
pub struct ImuIntState<'a> {
    pub input: Mutex<core::cell::RefCell<Option<Input<'a>>>>,
}

// Handle IMU interrupt events
#[esp_hal::ram]
pub fn handle_imu_int_generic(state: &ImuIntState, flag: &AtomicBool) {
    // Access IMU interrupt state within critical section
    critical_section::with(|cs| {
        // Check and clear interrupt
        let mut binding = state.input.borrow_ref_mut(cs);
        let pin = match binding.as_mut() {
            Some(p) => p,
            None => return,
        };
        if pin.is_interrupt_set() {
            pin.clear_interrupt();
            flag.store(true, core::sync::atomic::Ordering::Relaxed);
        }
    });
}
