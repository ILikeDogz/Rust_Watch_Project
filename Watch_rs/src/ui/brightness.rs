// Manage brightness state and flags for the UI.

use core::cell::RefCell;

use critical_section::Mutex;

static BRIGHTNESS_PCT: Mutex<RefCell<u8>> = Mutex::new(RefCell::new(100));
static BRIGHTNESS_EDIT: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static BRIGHTNESS_LAST: Mutex<RefCell<Option<u8>>> = Mutex::new(RefCell::new(None)); 
static BRIGHTNESS_DIRTY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false)); 

// Get the initial brightness percentage
pub fn get_brightness_pct() -> u8 {
    critical_section::with(|cs| *BRIGHTNESS_PCT.borrow(cs).borrow())
}

// Adjust brightness by delta, return new percentage
pub fn brightness_adjust(delta: i32) -> u8 {
    if delta == 0 {
        return get_brightness_pct();
    }
    critical_section::with(|cs| {
        let mut pct = *BRIGHTNESS_PCT.borrow(cs).borrow();
        let mut v = pct as i32 + delta;
        if v < 0 {
            v = 0;
        } else if v > 100 {
            v = 100;
        }
        pct = v as u8;
        // Mark dirty if changed
        if pct != *BRIGHTNESS_PCT.borrow(cs).borrow() {
            *BRIGHTNESS_PCT.borrow(cs).borrow_mut() = pct;
            *BRIGHTNESS_DIRTY.borrow(cs).borrow_mut() = true;
        }
        pct
    })
}

// Check if brightness edit mode is active
pub fn brightness_edit_active() -> bool {
    critical_section::with(|cs| *BRIGHTNESS_EDIT.borrow(cs).borrow())
}

// Set brightness edit mode active/inactive
pub fn brightness_edit_set(active: bool) {
    critical_section::with(|cs| *BRIGHTNESS_EDIT.borrow(cs).borrow_mut() = active);
}

// Take and clear the brightness dirty flag
pub fn brightness_take_dirty() -> bool {
    critical_section::with(|cs| {
        let mut d = BRIGHTNESS_DIRTY.borrow(cs).borrow_mut();
        let was = *d;
        *d = false;
        was
    })
}

// Get the last brightness percentage
pub fn get_brightness_last_pct() -> Option<u8> {
    critical_section::with(|cs| *BRIGHTNESS_LAST.borrow(cs).borrow())
}

// Set the last brightness percentage
pub fn set_brightness_last_pct(pct: Option<u8>) {
    critical_section::with(|cs| {
        *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = pct;
    });
}

// Reset the last brightness percentage
pub fn reset_brightness_last() {
    set_brightness_last_pct(None);
}

// Reset all brightness-related flags (call on cache clear)
pub fn reset_flags() {
    critical_section::with(|cs| {
        *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = None;
        *BRIGHTNESS_DIRTY.borrow(cs).borrow_mut() = false;
        *BRIGHTNESS_EDIT.borrow(cs).borrow_mut() = false;
    });
}
