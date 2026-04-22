// Application state management for inputs and UI

use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use critical_section::Mutex;
use esp_hal::gpio::Input;

use crate::input::{ButtonState, ImuIntState, RotaryState};
use crate::ui::{Page, UiState};
use esp_hal::gpio::Event;

// ---------------------------------------------------------------------------
// Terminal state (UART receive buffer for the UartTerminal page)
// ---------------------------------------------------------------------------

use heapless::{String as HString, Vec as HVec};

pub const TERM_COLS: usize = 16;
pub const TERM_ROWS: usize = 8;

pub type TermLine = HString<TERM_COLS>;
pub type TermLines = HVec<TermLine, TERM_ROWS>;

static TERM_LINES: Mutex<RefCell<TermLines>> = Mutex::new(RefCell::new(TermLines::new()));
static TERM_LINE_BUF: Mutex<RefCell<HString<256>>> = Mutex::new(RefCell::new(HString::new()));
static TERM_DIRTY: AtomicBool = AtomicBool::new(false);
static TERM_LINE_COUNT: AtomicU8 = AtomicU8::new(0);

fn flush_line_buf() {
    critical_section::with(|cs| {
        let mut lines = TERM_LINES.borrow(cs).borrow_mut();
        let mut buf = TERM_LINE_BUF.borrow(cs).borrow_mut();

        let buf_len = buf.len();
        // Copy content out to a fixed array so we can clear buf and still iterate
        let mut bytes = [0u8; 256];
        bytes[..buf_len].copy_from_slice(buf.as_bytes());
        buf.clear();

        if buf_len == 0 {
            // Empty line (bare \n)
            if lines.is_full() {
                lines.remove(0);
            }
            let _ = lines.push(TermLine::new());
        } else {
            let mut start = 0;
            while start < buf_len {
                let end = (start + TERM_COLS).min(buf_len);
                let mut line = TermLine::new();
                for &byte in &bytes[start..end] {
                    let _ = line.push(byte as char);
                }
                if lines.is_full() {
                    lines.remove(0);
                }
                let _ = lines.push(line);
                start = end;
            }
        }
        TERM_LINE_COUNT.store(lines.len() as u8, Ordering::Relaxed);
    });
    TERM_DIRTY.store(true, Ordering::Release);
}

pub fn terminal_push_byte(byte: u8) {
    if byte == b'\r' {
        return;
    }
    if byte == b'\n' {
        flush_line_buf();
        return;
    }
    // Only accept printable ASCII
    if byte < 0x20 || byte > 0x7E {
        return;
    }
    let was_full = critical_section::with(|cs| {
        let mut buf = TERM_LINE_BUF.borrow(cs).borrow_mut();
        // If buffer is at capacity, force-flush it before accepting this byte.
        if buf.len() >= buf.capacity() {
            return true;
        }
        let _ = buf.push(byte as char);
        false
    });
    if was_full {
        flush_line_buf();
        terminal_push_byte(byte);
    }
}

pub fn terminal_dirty() -> bool {
    TERM_DIRTY.load(Ordering::Acquire)
}

pub fn terminal_take_dirty() -> bool {
    TERM_DIRTY.swap(false, Ordering::Acquire)
}

pub fn terminal_line_count() -> usize {
    TERM_LINE_COUNT.load(Ordering::Relaxed) as usize
}

pub fn terminal_with_lines<F: FnOnce(&TermLines)>(f: F) {
    critical_section::with(|cs| f(&TERM_LINES.borrow(cs).borrow()));
}

pub fn terminal_reset() {
    critical_section::with(|cs| {
        TERM_LINES.borrow(cs).borrow_mut().clear();
        TERM_LINE_BUF.borrow(cs).borrow_mut().clear();
    });
    TERM_LINE_COUNT.store(0, Ordering::Relaxed);
    TERM_DIRTY.store(false, Ordering::Release);
}

static BUTTON1_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON2_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON3_PRESSED: AtomicBool = AtomicBool::new(false);
static IMU_INT_FLAG: AtomicBool = AtomicBool::new(false);
static INPUTS_ENABLED: AtomicBool = AtomicBool::new(false);

// Shared resources for Button
static BUTTON1: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button1",
};

static BUTTON2: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button2",
};

static BUTTON3: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button3",
};

// Shared resources for rotary encoder
static ROTARY: RotaryState<'static> = RotaryState {
    clk: Mutex::new(RefCell::new(None)),
    dt: Mutex::new(RefCell::new(None)),
    position: Mutex::new(Cell::new(0)),
    last_qstate: Mutex::new(Cell::new(0)), // bits: [CLK<<1 | DT]
    last_step: Mutex::new(Cell::new(0)),   // +1 or -1 from last transition
};

// IMU interrupt input holder
static IMU_INT: ImuIntState<'static> = ImuIntState {
    input: Mutex::new(RefCell::new(None)),
};

// Global UI state defaults to the UART terminal homepage.
static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState {
    page: Page::UartTerminal,
    dialog: None,
}));

// Install input pins and initialize their states
pub fn install_inputs(
    mut btn1: Input<'static>,
    mut btn2: Input<'static>,
    mut btn3: Input<'static>,
    mut enc_clk: Input<'static>,
    mut enc_dt: Input<'static>,
    mut imu_int: Option<Input<'static>>,
    now_ms: u64,
) {
    INPUTS_ENABLED.store(false, Ordering::Release);
    // Stop listening for interrupts during boot animation.
    // This prevents GPIO interrupts from firing before the handler is installed.
    btn1.unlisten();
    btn2.unlisten();
    btn3.unlisten();
    enc_clk.unlisten();
    enc_dt.unlisten();
    btn1.clear_interrupt();
    btn2.clear_interrupt();
    btn3.clear_interrupt();
    enc_clk.clear_interrupt();
    enc_dt.clear_interrupt();
    if let Some(pin) = imu_int.as_mut() {
        pin.unlisten();
        pin.clear_interrupt();
    }

    // Read initial levels
    let btn1_level = btn1.is_high();
    let btn2_level = btn2.is_high();
    let btn3_level = btn3.is_high();
    let clk_initial = enc_clk.is_high() as u8;
    let dt_initial = enc_dt.is_high() as u8;
    let qstate_initial = (clk_initial << 1) | dt_initial;

    // Store inputs and initial states
    critical_section::with(|cs| {
        BUTTON1.input.borrow_ref_mut(cs).replace(btn1);
        BUTTON1.last_level.borrow(cs).set(btn1_level);
        BUTTON1.last_interrupt.borrow(cs).set(now_ms);

        BUTTON2.input.borrow_ref_mut(cs).replace(btn2);
        BUTTON2.last_level.borrow(cs).set(btn2_level);
        BUTTON2.last_interrupt.borrow(cs).set(now_ms);

        BUTTON3.input.borrow_ref_mut(cs).replace(btn3);
        BUTTON3.last_level.borrow(cs).set(btn3_level);
        BUTTON3.last_interrupt.borrow(cs).set(now_ms);

        ROTARY.clk.borrow_ref_mut(cs).replace(enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(enc_dt);
        ROTARY.last_qstate.borrow(cs).set(qstate_initial);
        ROTARY.position.borrow(cs).set(0);
        ROTARY.last_step.borrow(cs).set(0);

        if let Some(pin) = imu_int {
            IMU_INT.input.borrow_ref_mut(cs).replace(pin);
        }
    });

    // Clear any pending events
    BUTTON1_PRESSED.store(false, Ordering::Release);
    BUTTON2_PRESSED.store(false, Ordering::Release);
    BUTTON3_PRESSED.store(false, Ordering::Release);
    IMU_INT_FLAG.store(false, Ordering::Release);
}

// Prime inputs by clearing pending state and enabling interrupts
pub fn prime_inputs(_now_ms: u64) {
    // Clear any pending state and re-enable interrupt listening.
    // This is called after the interrupt handler is installed but before
    // inputs are enabled. Set last_interrupt to 0 so the first button press
    // always passes the debounce check.
    critical_section::with(|cs| {
        if let Some(btn) = BUTTON1.input.borrow_ref_mut(cs).as_mut() {
            btn.clear_interrupt();
            BUTTON1.last_level.borrow(cs).set(btn.is_high());
            BUTTON1.last_interrupt.borrow(cs).set(0);
            btn.listen(Event::AnyEdge);
        }
        if let Some(btn) = BUTTON2.input.borrow_ref_mut(cs).as_mut() {
            btn.clear_interrupt();
            BUTTON2.last_level.borrow(cs).set(btn.is_high());
            BUTTON2.last_interrupt.borrow(cs).set(0);
            btn.listen(Event::AnyEdge);
        }
        if let Some(btn) = BUTTON3.input.borrow_ref_mut(cs).as_mut() {
            btn.clear_interrupt();
            BUTTON3.last_level.borrow(cs).set(btn.is_high());
            BUTTON3.last_interrupt.borrow(cs).set(0);
            btn.listen(Event::AnyEdge);
        }

        if let (Some(clk), Some(dt)) = (
            ROTARY.clk.borrow_ref_mut(cs).as_mut(),
            ROTARY.dt.borrow_ref_mut(cs).as_mut(),
        ) {
            clk.clear_interrupt();
            dt.clear_interrupt();
            let qstate = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
            ROTARY.last_qstate.borrow(cs).set(qstate);
            ROTARY.last_step.borrow(cs).set(0);
            clk.listen(Event::AnyEdge);
            dt.listen(Event::AnyEdge);
        }

        if let Some(pin) = IMU_INT.input.borrow_ref_mut(cs).as_mut() {
            pin.clear_interrupt();
            pin.listen(Event::AnyEdge);
        }
    });

    BUTTON1_PRESSED.store(false, Ordering::Release);
    BUTTON2_PRESSED.store(false, Ordering::Release);
    BUTTON3_PRESSED.store(false, Ordering::Release);
    IMU_INT_FLAG.store(false, Ordering::Release);
}

// Check if inputs are enabled
pub fn inputs_enabled() -> bool {
    INPUTS_ENABLED.load(Ordering::Acquire)
}

// Enable or disable input handling
pub fn set_inputs_enabled(enabled: bool) {
    INPUTS_ENABLED.store(enabled, Ordering::Release);
}

// Clear rotary encoder interrupts
pub fn clear_encoder_interrupts() {
    critical_section::with(|cs| {
        if let Some(clk) = ROTARY.clk.borrow_ref_mut(cs).as_mut() {
            clk.clear_interrupt();
        }
        if let Some(dt) = ROTARY.dt.borrow_ref_mut(cs).as_mut() {
            dt.clear_interrupt();
        }
    });
}

// Clear IMU interrupt
pub fn clear_imu_interrupt() {
    critical_section::with(|cs| {
        if let Some(pin) = IMU_INT.input.borrow_ref_mut(cs).as_mut() {
            pin.clear_interrupt();
        }
    });
    IMU_INT_FLAG.store(false, Ordering::Release);
}

// Get current UI state
pub fn ui_state_get() -> UiState {
    critical_section::with(|cs| UI_STATE.borrow(cs).get())
}

// Set UI state
pub fn ui_state_set(state: UiState) {
    critical_section::with(|cs| {
        UI_STATE.borrow(cs).set(state);
    });
}

// Update UI state with a function
pub fn ui_state_update(f: impl FnOnce(UiState) -> UiState) {
    critical_section::with(|cs| {
        let state = UI_STATE.borrow(cs).get();
        UI_STATE.borrow(cs).set(f(state));
    });
}

// Get rotary encoder position
pub fn rotary_position() -> i32 {
    critical_section::with(|cs| ROTARY.position.borrow(cs).get())
}

// Check if IMU interrupt pin is low
pub fn imu_int_pin_low() -> bool {
    critical_section::with(|cs| {
        IMU_INT
            .input
            .borrow_ref(cs)
            .as_ref()
            .map(|p| p.is_low())
            .unwrap_or(false)
    })
}

// Take and clear button 1 event flag
pub fn take_button1_event() -> bool {
    BUTTON1_PRESSED.swap(false, Ordering::Acquire)
}

// Take and clear button 2 event flag
pub fn take_button2_event() -> bool {
    BUTTON2_PRESSED.swap(false, Ordering::Acquire)
}

// Take and clear button 3 event flag
pub fn take_button3_event() -> bool {
    BUTTON3_PRESSED.swap(false, Ordering::Acquire)
}

// Take and clear IMU interrupt event flag
pub fn take_imu_int_flag() -> bool {
    IMU_INT_FLAG.swap(false, Ordering::Relaxed)
}

// Accessors for input button 1 state
pub fn button1() -> &'static ButtonState<'static> {
    &BUTTON1
}

// Accessors for input button 2 state
pub fn button2() -> &'static ButtonState<'static> {
    &BUTTON2
}

// Accessors for input button 3 state
pub fn button3() -> &'static ButtonState<'static> {
    &BUTTON3
}

// Accessor for rotary encoder state
pub fn rotary() -> &'static RotaryState<'static> {
    &ROTARY
}

// Accessor for IMU interrupt state
pub fn imu_int() -> &'static ImuIntState<'static> {
    &IMU_INT
}

// Accessor for button 1 event flag
pub fn button1_flag() -> &'static AtomicBool {
    &BUTTON1_PRESSED
}

// Accessor for button 2 event flag
pub fn button2_flag() -> &'static AtomicBool {
    &BUTTON2_PRESSED
}

// Accessor for button 3 event flag
pub fn button3_flag() -> &'static AtomicBool {
    &BUTTON3_PRESSED
}

// Accessor for IMU interrupt event flag
pub fn imu_int_flag() -> &'static AtomicBool {
    &IMU_INT_FLAG
}
