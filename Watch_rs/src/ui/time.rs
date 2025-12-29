use core::cell::RefCell;

use critical_section::{CriticalSection, Mutex};
use esp_hal::timer::systimer::{SystemTimer, Unit};

// Simple software clock: base seconds and ticks when set.
static CLOCK_BASE_SECS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));
static CLOCK_BASE_TICKS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ClockEditState {
    pub digits: [u8; 4], // HHMM digits
    pub idx: u8,         // active digit 0-3
}

#[derive(Copy, Clone, Default)]
pub struct HandCache {
    pub sec: Option<embedded_graphics::prelude::Point>,
    pub min: Option<embedded_graphics::prelude::Point>,
    pub hour: Option<embedded_graphics::prelude::Point>,
}

impl HandCache {
    pub const fn new() -> Self {
        Self {
            sec: None,
            min: None,
            hour: None,
        }
    }
}

static CLOCK_EDIT: Mutex<RefCell<Option<ClockEditState>>> = Mutex::new(RefCell::new(None));
static HAND_CACHE: Mutex<RefCell<HandCache>> = Mutex::new(RefCell::new(HandCache::new()));
static WATCH_FACE_DIRTY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

pub fn set_clock_seconds(seconds: u32) {
    // Set the software clock to the specified seconds since epoch
    let now = SystemTimer::unit_value(Unit::Unit0);
    critical_section::with(|cs| {
        *CLOCK_BASE_SECS.borrow(cs).borrow_mut() = seconds as u64;
        *CLOCK_BASE_TICKS.borrow(cs).borrow_mut() = now;
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
        *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
    });
}

pub fn watch_edit_active() -> bool {
    // Check if clock edit mode is active
    critical_section::with(|cs| CLOCK_EDIT.borrow(cs).borrow().is_some())
}

pub fn watch_edit_start() {
    // Initialize edit state with current time
    let now = clock_now_seconds();
    let total_mins = now / 60;
    let h = ((total_mins / 60) % 24) as u8;
    let m = (total_mins % 60) as u8;
    let digits = [h / 10, h % 10, m / 10, m % 10];

    // Set edit state
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = Some(ClockEditState { digits, idx: 0 });
    });
}

pub fn watch_edit_cancel() {
    // Clear edit state without committing changes
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = None;
    });
}

pub fn watch_edit_advance() {
    // Move to next digit or commit changes if on last digit
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        if let Some(mut ed) = *guard {
            if ed.idx < 3 {
                ed.idx += 1;
                *guard = Some(ed);
            } else {
                // Commit
                let hours = (ed.digits[0] as u32) * 10 + (ed.digits[1] as u32);
                let mins = (ed.digits[2] as u32) * 10 + (ed.digits[3] as u32);
                let secs = (hours * 60 + mins) * 60;
                set_clock_seconds(secs);
                *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
                *guard = None;
            }
        }
    });
}

pub fn watch_edit_adjust(delta: i32) {
    // Adjust the active digit by delta (+1 or -1)
    if delta == 0 {
        return;
    }
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        // Adjust active digit
        if let Some(mut ed) = *guard {
            let idx = ed.idx as usize;
            let mut digit = ed.digits[idx] as i32;
            // Determine min/max for digit
            let (min_d, max_d) = match idx {
                0 => (0, 2),
                1 => {
                    if ed.digits[0] == 2 {
                        (0, 3)
                    } else {
                        (0, 9)
                    }
                }
                2 => (0, 5),
                _ => (0, 9),
            };
            // Adjust digit
            digit += delta;
            // Wrap around
            if digit > max_d {
                digit = min_d;
            }
            if digit < min_d {
                digit = max_d;
            }

            // Update digit
            ed.digits[idx] = digit as u8;
            *guard = Some(ed);
        }
    });
}

pub fn current_edit_state() -> Option<ClockEditState> {
    critical_section::with(|cs| *CLOCK_EDIT.borrow(cs).borrow())
}

fn clock_now_seconds() -> u64 {
    // Get current software clock time in seconds since epoch
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second();
        let elapsed = now.saturating_sub(base_ticks) / tps;
        base_secs.saturating_add(elapsed)
    })
}

pub fn clock_now_seconds_u32() -> u32 {
    clock_now_seconds() as u32
}

pub fn clock_now_seconds_f32() -> f32 {
    // Get current software clock time in seconds since epoch as f32
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second() as u64;
        let elapsed_ticks = now.saturating_sub(base_ticks);
        let whole = elapsed_ticks / tps;
        let frac = (elapsed_ticks % tps) as f32 / tps as f32;
        // Work modulo 24h to preserve sub-second precision even with large epoch seconds.
        let total = base_secs + whole;
        let within_day = (total % 86_400) as f32;
        within_day + frac
    })
}

// Return hours, minutes, seconds as f32 with good precision by working modulo 12h.
pub fn clock_now_hms_f32() -> (f32, f32, f32) {
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second() as u64;
        let elapsed_ticks = now.saturating_sub(base_ticks);
        let whole = elapsed_ticks / tps;
        let frac = (elapsed_ticks % tps) as f32 / tps as f32;
        let total = base_secs + whole;
        let s = (total % 60) as f32 + frac;
        let m_total = total / 60;
        let m = (m_total % 60) as f32 + s / 60.0;
        let h_total = m_total / 60;
        let h = (h_total % 12) as f32 + m / 60.0;
        (h, m, s)
    })
}

// Format current clock as HH:MM into the provided 5-byte buffer and return it as &str.
pub fn format_clock_hm(buf: &mut [u8; 5]) -> &str {
    let total_secs = clock_now_seconds();
    let total_mins = total_secs / 60;
    let h = (total_mins / 60) % 24;
    let m = total_mins % 60;

    buf[0] = b'0' + (h / 10) as u8;
    buf[1] = b'0' + (h % 10) as u8;
    buf[2] = b':';
    buf[3] = b'0' + (m / 10) as u8;
    buf[4] = b'0' + (m % 10) as u8;

    core::str::from_utf8(buf).unwrap_or("??:??")
}

pub fn get_clock_seconds() -> u64 {
    clock_now_seconds()
}

pub fn reset_hand_cache() {
    critical_section::with(|cs| {
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
    });
}

pub fn with_hand_cache<R>(f: impl FnOnce(&mut HandCache) -> R) -> R {
    critical_section::with(|cs| f(&mut HAND_CACHE.borrow(cs).borrow_mut()))
}

pub fn hand_cache_mut<'cs>(cs: CriticalSection<'cs>) -> core::cell::RefMut<'cs, HandCache> {
    HAND_CACHE.borrow(cs).borrow_mut()
}

pub fn take_watch_face_dirty() -> bool {
    critical_section::with(|cs| {
        let mut f = WATCH_FACE_DIRTY.borrow(cs).borrow_mut();
        let dirty = *f;
        if dirty {
            *f = false;
        }
        dirty
    })
}

pub fn mark_watch_face_dirty() {
    critical_section::with(|cs| *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true);
}

pub fn reset_clock_state() {
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = None;
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
        *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = false;
    });
}
