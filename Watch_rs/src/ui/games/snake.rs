// Snake game logic and rendering.

extern crate alloc;

use alloc::format;
use core::cell::RefCell;

use critical_section::Mutex;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size},
    primitives::{PrimitiveStyle, Rectangle},
    Drawable,
};

use crate::ui::{draw::draw_text, games::common::draw_lines, CENTER, PanelRgb565, RESOLUTION};

const SNAKE_CELL_PX: i32 = 9;
const SNAKE_BOARD_PX: i32 = 324;
const SNAKE_GRID: i16 = (SNAKE_BOARD_PX / SNAKE_CELL_PX) as i16;
const SNAKE_MAX_LEN: usize = (SNAKE_GRID as usize) * (SNAKE_GRID as usize);
const SNAKE_START_LEN: usize = 4;
const SNAKE_STEP_MS: u64 = 140;

const GRASS_LIGHT: Rgb565 = Rgb565::new(0, 50, 0);
const GRASS_DARK: Rgb565 = Rgb565::new(0, 36, 0);
const SNAKE_COLOR: Rgb565 = Rgb565::BLUE;
const BORDER_COLOR: Rgb565 = Rgb565::WHITE;
const START_TEXT: &str = "Press to start";
const GAME_OVER_TEXT: &str = "Game Over";
const WIN_TEXT: &str = "You Win";
const RESTART_TEXT: &str = "Press to restart";
const SCORE_TEXT_PAD: i32 = 12;
const STATUS_TEXT_PAD: i32 = 8;
const STATUS_TEXT_MAX_CHARS: i32 = 16;
const SNAKE_TURN_STEPS: i32 = 2;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Dir {
    Up,
    Right,
    Down,
    Left,
}

#[derive(Copy, Clone)]
struct SnakeState {
    body: [(i16, i16); SNAKE_MAX_LEN],
    len: usize,
    dir: Dir,
    started: bool,
    game_over: bool,
    win: bool,
    last_step_ms: u64,
    bg_drawn: bool,
    snake_drawn: bool,
    last_tail: Option<(i16, i16)>,
    text_visible: bool,
    needs_clear_all: bool,
    apple: Option<(i16, i16)>,
    last_apple: Option<(i16, i16)>,
    score: u32,
    rng: u32,
    pending_steps: i32,
}

static SNAKE_STATE: Mutex<RefCell<SnakeState>> = Mutex::new(RefCell::new(SnakeState {
    body: [(0, 0); SNAKE_MAX_LEN],
    len: 0,
    dir: Dir::Right,
    started: false,
    game_over: false,
    win: false,
    last_step_ms: 0,
    bg_drawn: false,
    snake_drawn: false,
    last_tail: None,
    text_visible: false,
    needs_clear_all: false,
    apple: None,
    last_apple: None,
    score: 0,
    rng: 1,
    pending_steps: 0,
}));

fn board_origin() -> (i32, i32) {
    let x0 = (RESOLUTION as i32 - SNAKE_BOARD_PX) / 2;
    let y0 = (RESOLUTION as i32 - SNAKE_BOARD_PX) / 2;
    (x0, y0)
}

fn cell_top_left(x: i16, y: i16) -> (i32, i32) {
    let (x0, y0) = board_origin();
    (
        x0 + (x as i32 * SNAKE_CELL_PX),
        y0 + (y as i32 * SNAKE_CELL_PX),
    )
}

fn grass_color(x: i16, y: i16) -> Rgb565 {
    if ((x as i32 + y as i32) & 1) == 0 {
        GRASS_LIGHT
    } else {
        GRASS_DARK
    }
}

fn draw_cell(disp: &mut impl PanelRgb565, x: i16, y: i16, color: Rgb565) {
    let (px, py) = cell_top_left(x, y);
    let rect = Rectangle::new(
        Point::new(px, py),
        Size::new(SNAKE_CELL_PX as u32, SNAKE_CELL_PX as u32),
    );
    let _ = rect
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(disp);
}

fn draw_board(disp: &mut impl PanelRgb565) {
    let _ = disp.clear(Rgb565::BLACK);
    for y in 0..SNAKE_GRID {
        for x in 0..SNAKE_GRID {
            draw_cell(disp, x, y, grass_color(x, y));
        }
    }
    let (x0, y0) = board_origin();
    let border = Rectangle::new(
        Point::new(x0, y0),
        Size::new(SNAKE_BOARD_PX as u32, SNAKE_BOARD_PX as u32),
    );
    let _ = border
        .into_styled(PrimitiveStyle::with_stroke(BORDER_COLOR, 2))
        .draw(disp);
}

fn score_center_y() -> i32 {
    let (_, y0) = board_origin();
    (y0 - SCORE_TEXT_PAD).max(12)
}

fn reset_snake_state(state: &mut SnakeState, keep_bg: bool) {
    let cx = SNAKE_GRID / 2;
    let cy = SNAKE_GRID / 2;
    state.len = SNAKE_START_LEN.min(SNAKE_MAX_LEN);
    for i in 0..state.len {
        state.body[i] = (cx - i as i16, cy);
    }
    state.dir = Dir::Right;
    state.started = false;
    state.game_over = false;
    state.win = false;
    state.last_step_ms = 0;
    state.bg_drawn = keep_bg;
    state.snake_drawn = false;
    state.last_tail = None;
    state.text_visible = false;
    state.needs_clear_all = false;
    state.last_apple = None;
    state.score = 0;
    state.apple = spawn_apple(state);
    state.pending_steps = 0;
}

pub fn snake_reset_on_exit() {
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        reset_snake_state(&mut state, false);
    });
}

pub fn snake_start(now_ms: u64) -> bool {
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        if state.started && !state.game_over {
            return false;
        }
        let keep_bg = state.bg_drawn;
        let text_visible = state.text_visible;
        state.rng ^= now_ms as u32;
        reset_snake_state(&mut state, keep_bg);
        state.text_visible = text_visible;
        state.started = true;
        state.last_step_ms = now_ms;
        true
    })
}

pub fn snake_active() -> bool {
    critical_section::with(|cs| {
        let state = SNAKE_STATE.borrow(cs).borrow();
        state.started && !state.game_over && !state.win
    })
}

pub fn snake_turn_steps(delta_steps: i32) -> bool {
    if delta_steps == 0 {
        return false;
    }
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        let old_dir = state.dir;
        let mut dir = state.dir;
        state.pending_steps += delta_steps;
        while state.pending_steps.abs() >= SNAKE_TURN_STEPS {
            let step_dir = if state.pending_steps > 0 { 1 } else { -1 };
            dir = if step_dir > 0 {
                match dir {
                    Dir::Up => Dir::Right,
                    Dir::Right => Dir::Down,
                    Dir::Down => Dir::Left,
                    Dir::Left => Dir::Up,
                }
            } else {
                match dir {
                    Dir::Up => Dir::Left,
                    Dir::Left => Dir::Down,
                    Dir::Down => Dir::Right,
                    Dir::Right => Dir::Up,
                }
            };
            state.pending_steps -= step_dir * SNAKE_TURN_STEPS;
        }
        let changed = dir != old_dir;
        state.dir = dir;
        changed
    })
}

pub fn snake_turn_button(clockwise: bool) -> bool {
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        let old_dir = state.dir;
        state.pending_steps = 0;
        state.dir = if clockwise {
            match state.dir {
                Dir::Up => Dir::Right,
                Dir::Right => Dir::Down,
                Dir::Down => Dir::Left,
                Dir::Left => Dir::Up,
            }
        } else {
            match state.dir {
                Dir::Up => Dir::Left,
                Dir::Left => Dir::Down,
                Dir::Down => Dir::Right,
                Dir::Right => Dir::Up,
            }
        };
        state.dir != old_dir
    })
}

pub fn snake_update(now_ms: u64) -> bool {
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        if !state.started || state.game_over || state.win {
            return false;
        }
        if now_ms.saturating_sub(state.last_step_ms) < SNAKE_STEP_MS {
            return false;
        }
        state.last_step_ms = now_ms;
        let (dx, dy) = match state.dir {
            Dir::Up => (0, -1),
            Dir::Right => (1, 0),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
        };
        let head = state.body[0];
        let next = (head.0 + dx, head.1 + dy);
        if next.0 < 0 || next.1 < 0 || next.0 >= SNAKE_GRID || next.1 >= SNAKE_GRID {
            state.game_over = true;
            state.started = false;
            state.needs_clear_all = true;
            state.last_apple = state.apple;
            state.apple = None;
            return true;
        }
        for i in 0..state.len {
            if state.body[i] == next {
                state.game_over = true;
                state.started = false;
                state.needs_clear_all = true;
                state.last_apple = state.apple;
                state.apple = None;
                return true;
            }
        }
        let ate = state.apple == Some(next);
        let old_len = state.len;
        let new_len = if ate {
            (old_len + 1).min(SNAKE_MAX_LEN)
        } else {
            old_len
        };
        let tail = state.body[old_len - 1];
        for i in (1..new_len).rev() {
            let src = if i - 1 < old_len {
                state.body[i - 1]
            } else {
                tail
            };
            state.body[i] = src;
        }
        state.body[0] = next;
        state.len = new_len;
        state.last_tail = if ate { None } else { Some(tail) };
        if ate {
            state.score = state.score.saturating_add(1);
            state.apple = spawn_apple(&mut *state);
            if state.apple.is_none() {
                state.win = true;
                state.started = false;
                state.needs_clear_all = true;
                state.last_apple = None;
            }
        }
        true
    })
}

pub fn play_snake(disp: &mut impl PanelRgb565) {
    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        if state.len == 0 {
            reset_snake_state(&mut state, false);
        }
    });
    let (
        body_opt,
        head_pos,
        len,
        last_tail,
        started,
        game_over,
        draw_bg,
        draw_full,
        text_visible,
        clear_all,
        apple,
        last_apple,
        score,
        win,
    ) =
        critical_section::with(|cs| {
            let state = SNAKE_STATE.borrow(cs).borrow();
            let draw_full = !state.snake_drawn; // Calculate draw_full early

            // OPTIMIZATION: Only copy full body if needed for clear/redraw
            let body_opt = if draw_full || state.needs_clear_all {
                let mut body = [(0i16, 0i16); SNAKE_MAX_LEN];
                body[..state.len].copy_from_slice(&state.body[..state.len]);
                Some(body)
            } else {
                None
            };
            
            // Always grab head for optimized updates
            let head = state.body[0];

            (
                body_opt,
                head,
                state.len,
                state.last_tail,
                state.started,
                state.game_over,
                !state.bg_drawn,
                draw_full,
                state.text_visible,
                state.needs_clear_all,
                state.apple,
                state.last_apple,
                state.score,
                state.win,
            )
        });

    if draw_bg {
        draw_board(disp);
    }

    if clear_all {
        if let Some(body) = body_opt {
            for i in 0..len {
                let (x, y) = body[i];
                draw_cell(disp, x, y, grass_color(x, y));
            }
        }
        if let Some((ax, ay)) = last_apple.or(apple) {
            draw_cell(disp, ax, ay, grass_color(ax, ay));
        }
    }

    if started && text_visible {
        clear_status_text(disp);
    }
    
    // Main Rendering Logic
    if started && !game_over {
        if draw_full {
             // Full Redraw: Apple + Body
             if let Some((ax, ay)) = apple {
                 draw_cell(disp, ax, ay, Rgb565::RED);
             }
             if let Some(body) = body_opt {
                 for i in 0..len {
                     let (x, y) = body[i];
                     draw_cell(disp, x, y, SNAKE_COLOR);
                 }
             }
        } else {
             // Optimized Partial Redraw: Tail -> Head
             // Allows separate small flushes for changed areas
             
             // 1. Clear Tail (Flush 1)
             if let Some((tx, ty)) = last_tail {
                 draw_cell(disp, tx, ty, grass_color(tx, ty));
             }

             // 2. Draw Head (Flush 2)
             let (x, y) = head_pos;
             draw_cell(disp, x, y, SNAKE_COLOR);

             // 3. Apple (Flush 3 if needed - usually redundant if stationary but cheap)
             if let Some((ax, ay)) = apple {
                 draw_cell(disp, ax, ay, Rgb565::RED);
             }
        }
    }

    if !started || game_over || win {
        if win {
            draw_lines(
                disp,
                CENTER,
                CENTER,
                &[(WIN_TEXT, -10), (RESTART_TEXT, 10)],
                Rgb565::WHITE,
                Rgb565::BLACK,
            );
        } else if game_over {
            draw_lines(
                disp,
                CENTER,
                CENTER,
                &[(GAME_OVER_TEXT, -10), (RESTART_TEXT, 10)],
                Rgb565::WHITE,
                Rgb565::BLACK,
            );
        } else {
            draw_text(
                disp,
                START_TEXT,
                Rgb565::WHITE,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                false,
                true,
                None,
            );
        }
    }

    let score_text = format!("{:04}", score % 10000);
    draw_text(
        disp,
        &score_text,
        Rgb565::RED,
        Some(Rgb565::BLACK),
        CENTER,
        score_center_y(),
        false,
        true,
        None,
    );

    critical_section::with(|cs| {
        let mut state = SNAKE_STATE.borrow(cs).borrow_mut();
        if draw_bg {
            state.bg_drawn = true;
        }
        if started && draw_full {
            state.snake_drawn = true;
        }
        state.last_tail = None;
        state.text_visible = !started || game_over || win;
        if clear_all {
            state.needs_clear_all = false;
            state.snake_drawn = false;
            state.last_apple = None;
        }
    });
}

fn clear_status_text(disp: &mut impl PanelRgb565) {
    let text_w = STATUS_TEXT_MAX_CHARS * 10;
    let text_h = 20 * 2 + 6;
    let left = CENTER - (text_w / 2) - STATUS_TEXT_PAD;
    let top = CENTER - (text_h / 2) - STATUS_TEXT_PAD;
    let right = CENTER + (text_w / 2) + STATUS_TEXT_PAD;
    let bottom = CENTER + (text_h / 2) + STATUS_TEXT_PAD;

    let (x0, y0) = board_origin();
    let (x1, y1) = (x0 + SNAKE_BOARD_PX, y0 + SNAKE_BOARD_PX);
    let cl = left.clamp(x0, x1 - 1);
    let ct = top.clamp(y0, y1 - 1);
    let cr = right.clamp(x0, x1 - 1);
    let cb = bottom.clamp(y0, y1 - 1);

    let cell_left = ((cl - x0) / SNAKE_CELL_PX).clamp(0, SNAKE_GRID as i32 - 1) as i16;
    let cell_right = ((cr - x0) / SNAKE_CELL_PX).clamp(0, SNAKE_GRID as i32 - 1) as i16;
    let cell_top = ((ct - y0) / SNAKE_CELL_PX).clamp(0, SNAKE_GRID as i32 - 1) as i16;
    let cell_bottom = ((cb - y0) / SNAKE_CELL_PX).clamp(0, SNAKE_GRID as i32 - 1) as i16;

    for y in cell_top..=cell_bottom {
        for x in cell_left..=cell_right {
            draw_cell(disp, x, y, grass_color(x, y));
        }
    }
}

fn spawn_apple(state: &mut SnakeState) -> Option<(i16, i16)> {
    if state.len as i16 >= SNAKE_GRID * SNAKE_GRID {
        return None;
    }
    for _ in 0..64 {
        let (x, y) = random_cell(state);
        if !snake_contains(state, (x, y)) {
            return Some((x, y));
        }
    }
    for y in 0..SNAKE_GRID {
        for x in 0..SNAKE_GRID {
            if !snake_contains(state, (x, y)) {
                return Some((x, y));
            }
        }
    }
    None
}

fn random_cell(state: &mut SnakeState) -> (i16, i16) {
    state.rng = state
        .rng
        .wrapping_mul(1664525)
        .wrapping_add(1013904223);
    let x = (state.rng % SNAKE_GRID as u32) as i16;
    let y = ((state.rng >> 16) % SNAKE_GRID as u32) as i16;
    (x, y)
}

fn snake_contains(state: &SnakeState, pos: (i16, i16)) -> bool {
    for i in 0..state.len {
        if state.body[i] == pos {
            return true;
        }
    }
    false
}
