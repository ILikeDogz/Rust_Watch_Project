// Application state management

use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::Mutex;
use esp_hal::gpio::Input;

use crate::input::{ButtonState, ImuIntState, RotaryState};
use crate::ui::{MainMenuState, Page, UiState};

static BUTTON1_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON2_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON3_PRESSED: AtomicBool = AtomicBool::new(false);
static IMU_INT_FLAG: AtomicBool = AtomicBool::new(false);

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

// Global UI state
static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState {
    page: Page::Main(MainMenuState::Home),
    dialog: None,
}));

pub fn install_inputs(
    btn1: Input<'static>,
    btn2: Input<'static>,
    btn3: Input<'static>,
    enc_clk: Input<'static>,
    enc_dt: Input<'static>,
    imu_int: Option<Input<'static>>,
) {
    let clk_initial = enc_clk.is_high() as u8;
    let dt_initial = enc_dt.is_high() as u8;
    let qstate_initial = (clk_initial << 1) | dt_initial;

    critical_section::with(|cs| {
        BUTTON1.input.borrow_ref_mut(cs).replace(btn1);
        BUTTON1.last_level.borrow(cs).set(true);

        BUTTON2.input.borrow_ref_mut(cs).replace(btn2);
        BUTTON2.last_level.borrow(cs).set(true);

        BUTTON3.input.borrow_ref_mut(cs).replace(btn3);
        BUTTON3.last_level.borrow(cs).set(true);

        ROTARY.clk.borrow_ref_mut(cs).replace(enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(enc_dt);
        ROTARY.last_qstate.borrow(cs).set(qstate_initial);
        ROTARY.position.borrow(cs).set(0);
        ROTARY.last_step.borrow(cs).set(0);

        if let Some(pin) = imu_int {
            IMU_INT.input.borrow_ref_mut(cs).replace(pin);
        }
    });
}

pub fn ui_state_get() -> UiState {
    critical_section::with(|cs| UI_STATE.borrow(cs).get())
}

pub fn ui_state_set(state: UiState) {
    critical_section::with(|cs| {
        UI_STATE.borrow(cs).set(state);
    });
}

pub fn ui_state_update(f: impl FnOnce(UiState) -> UiState) {
    critical_section::with(|cs| {
        let state = UI_STATE.borrow(cs).get();
        UI_STATE.borrow(cs).set(f(state));
    });
}

pub fn rotary_position() -> i32 {
    critical_section::with(|cs| ROTARY.position.borrow(cs).get())
}

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

pub fn take_button1_event() -> bool {
    BUTTON1_PRESSED.swap(false, Ordering::Acquire)
}

pub fn take_button2_event() -> bool {
    BUTTON2_PRESSED.swap(false, Ordering::Acquire)
}

pub fn take_button3_event() -> bool {
    BUTTON3_PRESSED.swap(false, Ordering::Acquire)
}

pub fn take_imu_int_flag() -> bool {
    IMU_INT_FLAG.swap(false, Ordering::Relaxed)
}

pub fn button1() -> &'static ButtonState<'static> {
    &BUTTON1
}

pub fn button2() -> &'static ButtonState<'static> {
    &BUTTON2
}

pub fn button3() -> &'static ButtonState<'static> {
    &BUTTON3
}

pub fn rotary() -> &'static RotaryState<'static> {
    &ROTARY
}

pub fn imu_int() -> &'static ImuIntState<'static> {
    &IMU_INT
}

pub fn button1_flag() -> &'static AtomicBool {
    &BUTTON1_PRESSED
}

pub fn button2_flag() -> &'static AtomicBool {
    &BUTTON2_PRESSED
}

pub fn button3_flag() -> &'static AtomicBool {
    &BUTTON3_PRESSED
}

pub fn imu_int_flag() -> &'static AtomicBool {
    &IMU_INT_FLAG
}
