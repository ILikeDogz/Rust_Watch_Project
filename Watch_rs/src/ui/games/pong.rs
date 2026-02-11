// Pong game logic and state.

extern crate alloc;

use alloc::format;
use embedded_graphics::primitives::PrimitiveStyle;
use core::cell::{Cell, RefCell};

use critical_section::Mutex;

use crate::display::FastPanelOps;
use crate::ui::{CENTER, PanelRgb565, RESOLUTION, draw::{draw_ring_segment_raw, draw_ring_segment_raw_fb_no_flush}, games::common::{clear_box, clear_box_fb, draw_lines, draw_lines_fb}};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size},
    primitives::Rectangle,
    Drawable,
};

// Step size for paddle angle adjustment
const PONG_STEP_DEG: f32 = 12.0;
pub const PONG_PADDLE_ARC_DEG: f32 = 32.0;
pub const PONG_PADDLE_THICKNESS: i32 = 10;
pub const PONG_PADDLE_STROKE: u8 = 3;
pub const PONG_PADDLE_RADIUS_PAD: i32 = 0;
pub const PONG_BALL_RADIUS: i32 = 4;
const PONG_BALL_SPEED_PX_S: f32 = 140.0;
const PONG_BALL_START_OFFSET_DEG: f32 = 25.0;
const PONG_BALL_START_JITTER_DEG: f32 = 5.0;
const PONG_BOUNCE_JITTER_DEG: f32 = 35.0;
const PONG_PADDLE_SPEED_BOOST: f32 = 0.2;
const PONG_PADDLE_SPEED_MIN: f32 = 0.75;
const PONG_PADDLE_SPEED_MAX: f32 = 1.25;
const PONG_BOUNCE_SPEEDS: [f32; 5] = [110.0, 130.0, 150.0, 170.0, 190.0];
const PONG_BALL_ATTACH_PAD: i32 = 1;
const PONG_PADDLE_HIT_PAD_DEG: f32 = 4.0;
const PONG_BALL_GRACE_MS: u64 = 120;
const PONG_BALL_BOUNCE_COOLDOWN_MS: u64 = 80;
const PONG_START_SCORE: u32 = 0;

static PONG_PADDLE_ANGLE: Mutex<RefCell<f32>> = Mutex::new(RefCell::new(270.0));
static PONG_PADDLE_LAST_ANGLE: Mutex<RefCell<Option<f32>>> = Mutex::new(RefCell::new(None));
static PONG_BALL_ACTIVE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static PONG_BALL_POS: Mutex<RefCell<(f32, f32)>> = Mutex::new(RefCell::new((0.0, 0.0)));
static PONG_BALL_VEL: Mutex<RefCell<(f32, f32)>> = Mutex::new(RefCell::new((0.0, 0.0)));
static PONG_BALL_LAST_UPDATE_MS: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));
static PONG_BALL_START_MS: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));
static PONG_BALL_LAST_BOUNCE_MS: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));
static PONG_BALL_LAST_POS: Mutex<RefCell<Option<(i32, i32)>>> =
    Mutex::new(RefCell::new(None));
static PONG_RNG: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static PONG_TEXT_VISIBLE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static PONG_GAME_OVER: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static PONG_WIN: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static PONG_SCORE: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

// Paddle angle accessors and mutators
pub fn pong_paddle_angle() -> f32 {
    critical_section::with(|cs| *PONG_PADDLE_ANGLE.borrow(cs).borrow())
}

// Last paddle angle accessors and mutators
pub fn pong_paddle_last_angle() -> Option<f32> {
    critical_section::with(|cs| *PONG_PADDLE_LAST_ANGLE.borrow(cs).borrow())
}

// Set last paddle angle
pub fn pong_paddle_set_last_angle(angle: Option<f32>) {
    critical_section::with(|cs| {
        *PONG_PADDLE_LAST_ANGLE.borrow(cs).borrow_mut() = angle;
    });
}

// Adjust paddle angle by delta steps
pub fn pong_paddle_adjust_timed(delta_steps: i32, _now_ms: u64) -> bool {
    if delta_steps == 0 {
        return false;
    }
    critical_section::with(|cs| {
        let mut angle = PONG_PADDLE_ANGLE.borrow(cs).borrow_mut();
        let mut next = *angle + (delta_steps as f32) * PONG_STEP_DEG;
        while next < 0.0 {
            next += 360.0;
        }
        while next >= 360.0 {
            next -= 360.0;
        }
        *angle = next;
    });
    true
}

// Flip paddle angle by 180 degrees
pub fn pong_paddle_flip_timed(_now_ms: u64) -> bool {
    critical_section::with(|cs| {
        let mut angle = PONG_PADDLE_ANGLE.borrow(cs).borrow_mut();
        let mut next = *angle + 180.0;
        if next >= 360.0 {
            next -= 360.0;
        }
        *angle = next;
    });
    true
}

// Compute play radius for pong game
pub fn pong_play_radius() -> i32 {
    (RESOLUTION as i32 / 2) - PONG_PADDLE_RADIUS_PAD
}

// Ball state accessors and mutators
pub fn pong_ball_active() -> bool {
    critical_section::with(|cs| PONG_BALL_ACTIVE.borrow(cs).get())
}

// Get current pong ball position
pub fn pong_ball_pos() -> (f32, f32) {
    critical_section::with(|cs| *PONG_BALL_POS.borrow(cs).borrow())
}

// Set current pong ball position
pub fn pong_ball_set_pos(pos: (f32, f32)) {
    critical_section::with(|cs| {
        *PONG_BALL_POS.borrow(cs).borrow_mut() = pos;
    });
}

// Get last pong ball position
pub fn pong_ball_last_pos() -> Option<(i32, i32)> {
    critical_section::with(|cs| *PONG_BALL_LAST_POS.borrow(cs).borrow())
}

// Set last pong ball position
pub fn pong_ball_set_last_pos(pos: Option<(i32, i32)>) {
    critical_section::with(|cs| {
        *PONG_BALL_LAST_POS.borrow(cs).borrow_mut() = pos;
    });
}

// Pong text visibility accessors and mutators
pub fn pong_text_visible() -> bool {
    critical_section::with(|cs| PONG_TEXT_VISIBLE.borrow(cs).get())
}

// Set pong text visibility
pub fn pong_set_text_visible(visible: bool) {
    critical_section::with(|cs| {
        PONG_TEXT_VISIBLE.borrow(cs).set(visible);
    });
}

// Game over state accessors and mutators
pub fn pong_game_over() -> bool {
    critical_section::with(|cs| PONG_GAME_OVER.borrow(cs).get())
}

// Set game over state
pub fn pong_set_game_over(active: bool) {
    critical_section::with(|cs| {
        PONG_GAME_OVER.borrow(cs).set(active);
    });
}

// Win state accessors and mutators
pub fn pong_win() -> bool {
    critical_section::with(|cs| PONG_WIN.borrow(cs).get())
}

// Set win state
pub fn pong_set_win(active: bool) {
    critical_section::with(|cs| {
        PONG_WIN.borrow(cs).set(active);
    });
}

// Score accessors and mutators
pub fn pong_score() -> u32 {
    critical_section::with(|cs| PONG_SCORE.borrow(cs).get())
}

// Reset score to zero
pub fn pong_score_reset() {
    critical_section::with(|cs| {
        PONG_SCORE.borrow(cs).set(0);
    });
}

// Increment score by one, capped at 99
fn pong_score_inc() -> u32 {
    critical_section::with(|cs| {
        let v = PONG_SCORE.borrow(cs).get();
        let next = (v.saturating_add(1)).min(99);
        PONG_SCORE.borrow(cs).set(next);
        next
    })
}

// Wrap angle to [0, 360)
fn wrap_angle_deg(mut ang: f32) -> f32 {
    while ang < 0.0 {
        ang += 360.0;
    }
    while ang >= 360.0 {
        ang -= 360.0;
    }
    ang
}

// Compute minimal difference between two angles in degrees
fn angle_diff_deg(a: f32, b: f32) -> f32 {
    let mut diff = (a - b).abs();
    if diff > 180.0 {
        diff = 360.0 - diff;
    }
    diff
}

fn pong_rand_u32(now_ms: u64) -> u32 {
    critical_section::with(|cs| {
        let state = PONG_RNG.borrow(cs);
        let mut v = state.get();
        if v == 0 {
            v = (now_ms as u32) ^ 0xA5A5_5A5A;
        }
        v = v.wrapping_mul(1664525).wrapping_add(1013904223);
        state.set(v);
        v
    })
}

fn angle_delta_deg(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

// Compute pong ball position attached to paddle
pub fn pong_ball_attached_pos(
    center_x: i32,
    center_y: i32,
    paddle_angle: f32,
    play_radius: i32,
) -> (f32, f32) {
    let attach_r = (play_radius
        - PONG_PADDLE_THICKNESS
        - (PONG_PADDLE_STROKE as i32 / 2)
        - PONG_BALL_RADIUS
        - PONG_BALL_ATTACH_PAD)
        .max(2);
    let ang = paddle_angle.to_radians();
    let cos = libm::cosf(ang);
    let sin = libm::sinf(ang);
    (
        center_x as f32 + cos * attach_r as f32,
        center_y as f32 + sin * attach_r as f32,
    )
}

// Start pong ball movement from paddle
pub fn pong_ball_start(now_ms: u64, paddle_angle: f32, play_radius: i32) -> bool {
    let center_x = CENTER;
    let center_y = CENTER;
    let pos = pong_ball_attached_pos(center_x, center_y, paddle_angle, play_radius);
    let jitter_range = (PONG_BALL_START_JITTER_DEG * 2.0) as u32;
    let jitter = (pong_rand_u32(now_ms) % (jitter_range + 1)) as f32 - PONG_BALL_START_JITTER_DEG;
    let dir = wrap_angle_deg(
        paddle_angle + 180.0 + PONG_BALL_START_OFFSET_DEG + jitter,
    )
    .to_radians();
    let vx = libm::cosf(dir) * PONG_BALL_SPEED_PX_S;
    let vy = libm::sinf(dir) * PONG_BALL_SPEED_PX_S;

    critical_section::with(|cs| {
        PONG_BALL_ACTIVE.borrow(cs).set(true);
        PONG_GAME_OVER.borrow(cs).set(false);
        PONG_WIN.borrow(cs).set(false);
        PONG_SCORE.borrow(cs).set(PONG_START_SCORE);
        *PONG_BALL_POS.borrow(cs).borrow_mut() = pos;
        *PONG_BALL_VEL.borrow(cs).borrow_mut() = (vx, vy);
        PONG_BALL_LAST_UPDATE_MS.borrow(cs).set(now_ms);
        PONG_BALL_START_MS.borrow(cs).set(now_ms);
        PONG_BALL_LAST_BOUNCE_MS.borrow(cs).set(0);
        *PONG_BALL_LAST_POS.borrow(cs).borrow_mut() = None;
    });
    true
}

// Update pong ball position and handle collisions
pub fn pong_ball_update(
    now_ms: u64,
    play_radius: i32,
    paddle_angle: f32,
    center_x: i32,
    center_y: i32,
) -> bool {
    if !pong_ball_active() {
        return false;
    }

    // Load current ball state
    let (mut x, mut y, mut vx, mut vy, last_ms, start_ms, last_bounce_ms) =
        critical_section::with(|cs| {
            let (x, y) = *PONG_BALL_POS.borrow(cs).borrow();
            let (vx, vy) = *PONG_BALL_VEL.borrow(cs).borrow();
            let last_ms = PONG_BALL_LAST_UPDATE_MS.borrow(cs).get();
            let start_ms = PONG_BALL_START_MS.borrow(cs).get();
            let last_bounce_ms = PONG_BALL_LAST_BOUNCE_MS.borrow(cs).get();
            (x, y, vx, vy, last_ms, start_ms, last_bounce_ms)
        });

    // Update position based on velocity and time delta
    let dt_ms = now_ms.saturating_sub(last_ms);
    if dt_ms == 0 {
        return false;
    }
    let dt = dt_ms as f32 / 1000.0;
    x += vx * dt;
    y += vy * dt;

    let dx = x - center_x as f32;
    let dy = y - center_y as f32;
    let dist = libm::sqrtf(dx * dx + dy * dy);
    let max_r = (play_radius
        - PONG_PADDLE_THICKNESS
        - (PONG_PADDLE_STROKE as i32 / 2)
        - PONG_BALL_RADIUS
        - 2)
        .max(1) as f32;

    // Handle collision with paddle
    let mut active = true;
    if dist >= max_r {
        let impact_ang = wrap_angle_deg(libm::atan2f(dy, dx).to_degrees());
        let within_paddle = angle_diff_deg(impact_ang, paddle_angle)
            <= (PONG_PADDLE_ARC_DEG / 2.0 + PONG_PADDLE_HIT_PAD_DEG);

        if within_paddle {
            if now_ms.saturating_sub(start_ms) >= PONG_BALL_GRACE_MS
                && now_ms.saturating_sub(last_bounce_ms) >= PONG_BALL_BOUNCE_COOLDOWN_MS
            {
                let score = pong_score_inc();
                critical_section::with(|cs| {
                    PONG_BALL_LAST_BOUNCE_MS.borrow(cs).set(now_ms);
                });
                if score >= 99 {
                    active = false;
                    critical_section::with(|cs| {
                        PONG_WIN.borrow(cs).set(true);
                        PONG_GAME_OVER.borrow(cs).set(false);
                        PONG_SCORE.borrow(cs).set(PONG_START_SCORE);
                    });
                }
            }

            // Compute new ball velocity after bounce
            let offset = angle_delta_deg(impact_ang, paddle_angle)
                .clamp(-(PONG_PADDLE_ARC_DEG / 2.0), PONG_PADDLE_ARC_DEG / 2.0);
            let offset_norm = (offset / (PONG_PADDLE_ARC_DEG / 2.0)).clamp(-1.0, 1.0);
            let speed_scale = (1.0 + (offset_norm * PONG_PADDLE_SPEED_BOOST))
                .clamp(PONG_PADDLE_SPEED_MIN, PONG_PADDLE_SPEED_MAX);
            let idx = (pong_rand_u32(now_ms) as usize) % PONG_BOUNCE_SPEEDS.len();
            let target_speed = PONG_BOUNCE_SPEEDS[idx] * speed_scale;
            let jitter_range = (PONG_BOUNCE_JITTER_DEG * 2.0) as u32;
            let jitter = (pong_rand_u32(now_ms) % (jitter_range + 1)) as f32
                - PONG_BOUNCE_JITTER_DEG;
            let out_ang = wrap_angle_deg(paddle_angle + 180.0 + jitter).to_radians();
            vx = libm::cosf(out_ang) * target_speed;
            vy = libm::sinf(out_ang) * target_speed;
            let nx = dx / dist.max(1.0);
            let ny = dy / dist.max(1.0);
            x = center_x as f32 + nx * max_r;
            y = center_y as f32 + ny * max_r;
        } else {
            active = false;
            let pos = pong_ball_attached_pos(center_x, center_y, paddle_angle, play_radius);
            x = pos.0;
            y = pos.1;
            critical_section::with(|cs| {
                PONG_GAME_OVER.borrow(cs).set(true);
            });
        }
    }

    // Save updated ball state
    critical_section::with(|cs| {
        PONG_BALL_ACTIVE.borrow(cs).set(active);
        *PONG_BALL_POS.borrow(cs).borrow_mut() = (x, y);
        *PONG_BALL_VEL.borrow(cs).borrow_mut() = (vx, vy);
        PONG_BALL_LAST_UPDATE_MS.borrow(cs).set(now_ms);
        if !active {
            PONG_TEXT_VISIBLE.borrow(cs).set(false);
        }
    });
    true
}

// Reset pong game state on exit
pub fn pong_reset_on_exit() {
    pong_paddle_set_last_angle(None);
    pong_ball_set_last_pos(None);
    critical_section::with(|cs| {
        PONG_BALL_ACTIVE.borrow(cs).set(false);
        PONG_BALL_LAST_UPDATE_MS.borrow(cs).set(0);
        *PONG_BALL_POS.borrow(cs).borrow_mut() = (0.0, 0.0);
        *PONG_BALL_VEL.borrow(cs).borrow_mut() = (0.0, 0.0);
        PONG_TEXT_VISIBLE.borrow(cs).set(false);
        PONG_GAME_OVER.borrow(cs).set(false);
        PONG_WIN.borrow(cs).set(false);
        PONG_SCORE.borrow(cs).set(PONG_START_SCORE);
    });
}

pub fn play_pong(disp: &mut impl PanelRgb565) {

    // Setup constants and state
    let angle = pong_paddle_angle();
    let last_angle = pong_paddle_last_angle();

    let center_x = CENTER;
    let center_y = CENTER;
    let radius = pong_play_radius();
    let thickness = PONG_PADDLE_THICKNESS;
    let stroke = PONG_PADDLE_STROKE;
    let arc_span = PONG_PADDLE_ARC_DEG;
    let ball_r = PONG_BALL_RADIUS;
    let clear_r = ball_r + 2;
    let fg = Rgb565::WHITE;
    let bg = Rgb565::BLACK;
    let halo_color = Rgb565::new(8, 8, 8);
    let halo_radius = (radius - 1).max(1);
    let halo_thickness = (thickness - 2).max(1);
    let halo_stroke = stroke.saturating_sub(1).max(1);
    let ball_color = Rgb565::BLUE;
    let text_half_w = 120;
    let text_half_h = 22;
    let score_y = CENTER - 100;
    let score_half_w = 30;
    let score_half_h = 10;
    let score_color = Rgb565::RED;

    let start = angle - (arc_span / 2.0);
    let end = angle + (arc_span / 2.0);

    // Resolve ball position, attach to paddle when idle.
    let active = pong_ball_active();
    let ball_pos = if active {
        pong_ball_pos()
    } else {
        let pos = pong_ball_attached_pos(center_x, center_y, angle, radius);
        pong_ball_set_pos(pos);
        pos
    };
    let ball_x = if ball_pos.0 >= 0.0 {
        (ball_pos.0 + 0.5) as i32
    } else {
        (ball_pos.0 - 0.5) as i32
    };
    let ball_y = if ball_pos.1 >= 0.0 {
        (ball_pos.1 + 0.5) as i32
    } else {
        (ball_pos.1 - 0.5) as i32
    };
    let last_ball = pong_ball_last_pos();
    let force_paddle_redraw = true;

    // Fast path: framebuffer-backed draw with a single flush.
    if let Some(co_display) =
        (disp as &mut dyn core::any::Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let mut bbox: Option<(i32, i32, i32, i32)> = None;
        let merge_bbox =
            |bbox: &mut Option<(i32, i32, i32, i32)>, b: Option<(i32, i32, i32, i32)>| {
                if let Some((x0, y0, x1, y1)) = b {
                    *bbox = Some(match *bbox {
                        Some((bx0, by0, bx1, by1)) => (
                            bx0.min(x0),
                            by0.min(y0),
                            bx1.max(x1),
                            by1.max(y1),
                        ),
                        None => (x0, y0, x1, y1),
                    });
                }
            };

        if last_angle.is_none() {
            let _ = FastPanelOps::fill_rect_fb(
                co_display,
                0,
                0,
                (RESOLUTION - 1) as i32,
                (RESOLUTION - 1) as i32,
                Rgb565::BLACK,
            );
            bbox = Some((0, 0, (RESOLUTION - 1) as i32, (RESOLUTION - 1) as i32));
        }

        // Clear the swept ball area between last and current positions.
        if let Some((px, py)) = last_ball {
            let min_x = px.min(ball_x);
            let max_x = px.max(ball_x);
            let min_y = py.min(ball_y);
            let max_y = py.max(ball_y);
            let x0 = (min_x - clear_r).max(0);
            let y0 = (min_y - clear_r).max(0);
            let x1 = (max_x + clear_r).min((RESOLUTION - 1) as i32);
            let y1 = (max_y + clear_r).min((RESOLUTION - 1) as i32);
            let _ = FastPanelOps::fill_rect_fb(
                co_display, x0, y0, x1, y1, Rgb565::BLACK,
            );
            merge_bbox(&mut bbox, Some((x0, y0, x1, y1)));
        }

        // Text/score are always cleared and redrawn on this pass.
        if active {
            let bb = clear_box_fb(
                co_display,
                center_x,
                center_y,
                text_half_w,
                text_half_h,
                Rgb565::BLACK,
            );
            merge_bbox(&mut bbox, Some(bb));
            pong_set_text_visible(false);
        } else {
            let win = pong_win();
            let game_over = pong_game_over();
            let line1 = if win {
                "You Win"
            } else if game_over {
                "Game Over"
            } else {
                "Press to start"
            };
            let line2 = if win || game_over {
                "Press to restart"
            } else {
                ""
            };
            let bb = clear_box_fb(
                co_display,
                center_x,
                center_y,
                text_half_w,
                text_half_h,
                Rgb565::BLACK,
            );
            if !line2.is_empty() {
                draw_lines_fb(
                    co_display,
                    center_x,
                    center_y,
                    &[(line1, -10), (line2, 10)],
                    Rgb565::WHITE,
                    Rgb565::BLACK,
                );
            } else {
                draw_lines_fb(
                    co_display,
                    center_x,
                    center_y,
                    &[(line1, 0)],
                    Rgb565::WHITE,
                    Rgb565::BLACK,
                );
            }
            merge_bbox(&mut bbox, Some(bb));
            pong_set_text_visible(true);
        }

        // Score counter (red) above center.
        let score = pong_score();
        let score_text = format!("{:02}", score % 100);
        let bb = clear_box_fb(
            co_display,
            center_x,
            score_y,
            score_half_w,
            score_half_h,
            Rgb565::BLACK,
        );
        draw_lines_fb(
            co_display,
            center_x,
            score_y,
            &[(&score_text, 0)],
            score_color,
            Rgb565::BLACK,
        );
        merge_bbox(&mut bbox, Some(bb));

        // Clear previous paddle arc and draw current arc.
        if let Some(prev) = last_angle {
            if (prev - angle).abs() > 0.01 || force_paddle_redraw {
                let prev_start = prev - (arc_span / 2.0);
                let prev_end = prev + (arc_span / 2.0);
                merge_bbox(&mut bbox, draw_ring_segment_raw_fb_no_flush(
                    co_display,
                    center_x,
                    center_y,
                    halo_radius,
                    halo_thickness,
                    halo_stroke,
                    prev_start,
                    prev_end,
                    bg,
                ));
                merge_bbox(&mut bbox, draw_ring_segment_raw_fb_no_flush(
                    co_display,
                    center_x,
                    center_y,
                    radius,
                    thickness,
                    stroke,
                    prev_start,
                    prev_end,
                    bg,
                ));
            }
        }

        // Draw current paddle arc
        if last_angle.is_none()
            || (last_angle.unwrap_or(angle) - angle).abs() > 0.01
            || force_paddle_redraw
        {
            merge_bbox(&mut bbox, draw_ring_segment_raw_fb_no_flush(
                co_display,
                center_x,
                center_y,
                halo_radius,
                halo_thickness,
                halo_stroke,
                start,
                end,
                halo_color,
            ));
            merge_bbox(&mut bbox, draw_ring_segment_raw_fb_no_flush(
                co_display,
                center_x,
                center_y,
                radius,
                thickness,
                stroke,
                start,
                end,
                fg,
            ));
        }

        // Draw ball last so it stays above text/score.
        let bx0 = (ball_x - ball_r).max(0);
        let by0 = (ball_y - ball_r).max(0);
        let bx1 = (ball_x + ball_r).min((RESOLUTION - 1) as i32);
        let by1 = (ball_y + ball_r).min((RESOLUTION - 1) as i32);
        let _ = FastPanelOps::fill_rect_fb(
            co_display, bx0, by0, bx1, by1, ball_color,
        );
        merge_bbox(&mut bbox, Some((bx0, by0, bx1, by1)));
        pong_ball_set_last_pos(Some((ball_x, ball_y)));

        if let Some((x0, y0, x1, y1)) = bbox {
            let fx0 = (x0.clamp(0, (RESOLUTION - 1) as i32) & !1) as u16;
            let fy0 = (y0.clamp(0, (RESOLUTION - 1) as i32) & !1) as u16;
            let fx1 = (x1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32)
                as u16;
            let fy1 = (y1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32)
                as u16;
            let _ = FastPanelOps::flush_rect_even(co_display, fx0, fy0, fx1, fy1);
        }

        pong_paddle_set_last_angle(Some(angle));
        return;
    }

    // Fallback path (non-FB): use embedded-graphics primitives.
    // Clear entire screen if first time drawing
    if last_angle.is_none() {
        let _ = disp.clear(Rgb565::BLACK);
    }

    if let Some((px, py)) = last_ball {
        let min_x = px.min(ball_x);
        let max_x = px.max(ball_x);
        let min_y = py.min(ball_y);
        let max_y = py.max(ball_y);
        let _ = Rectangle::new(
            Point::new(min_x - clear_r, min_y - clear_r),
            Size::new(
                ((max_x - min_x) + clear_r * 2 + 1) as u32,
                ((max_y - min_y) + clear_r * 2 + 1) as u32,
            ),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(disp);
    }

    // Text/score are always cleared and redrawn on this pass.
    if active {
        clear_box(
            disp,
            center_x,
            center_y,
            text_half_w,
            text_half_h,
            Rgb565::BLACK,
        );
        pong_set_text_visible(false);
    } else {
        let win = pong_win();
        let game_over = pong_game_over();
        let line1 = if win {
            "You Win"
        } else if game_over {
            "Game Over"
        } else {
            "Press to start"
        };
        let line2 = if win || game_over {
            "Press to restart"
        } else {
            ""
        };
        clear_box(
            disp,
            center_x,
            center_y,
            text_half_w,
            text_half_h,
            Rgb565::BLACK,
        );
        if !line2.is_empty() {
            draw_lines(
                disp,
                center_x,
                center_y,
                &[(line1, -10), (line2, 10)],
                Rgb565::WHITE,
                Rgb565::BLACK,
            );
        } else {
            draw_lines(
                disp,
                center_x,
                center_y,
                &[(line1, 0)],
                Rgb565::WHITE,
                Rgb565::BLACK,
            );
        }
        pong_set_text_visible(true);
    }

    // Score counter (red) above center.
    let score = pong_score();
    let score_text = format!("{:02}", score % 100);
    clear_box(
        disp,
        center_x,
        score_y,
        score_half_w,
        score_half_h,
        Rgb565::BLACK,
    );
    draw_lines(
        disp,
        center_x,
        score_y,
        &[(&score_text, 0)],
        score_color,
        Rgb565::BLACK,
    );

    // Clear previous paddle arc and draw current arc.
    if let Some(prev) = last_angle {
        if (prev - angle).abs() > 0.01 || force_paddle_redraw {
            let prev_start = prev - (arc_span / 2.0);
            let prev_end = prev + (arc_span / 2.0);
            draw_ring_segment_raw(
                disp,
                center_x,
                center_y,
                halo_radius,
                halo_thickness,
                halo_stroke,
                prev_start,
                prev_end,
                bg,
            );
            draw_ring_segment_raw(
                disp,
                center_x,
                center_y,
                radius,
                thickness,
                stroke,
                prev_start,
                prev_end,
                bg,
            );
        }
    }

    // Draw current paddle arc
    if last_angle.is_none()
        || (last_angle.unwrap_or(angle) - angle).abs() > 0.01
        || force_paddle_redraw
    {
        draw_ring_segment_raw(
            disp,
            center_x,
            center_y,
            halo_radius,
            halo_thickness,
            halo_stroke,
            start,
            end,
            halo_color,
        );
        draw_ring_segment_raw(
            disp,
            center_x,
            center_y,
            radius,
            thickness,
            stroke,
            start,
            end,
            fg,
        );
    }

    // Draw ball last so it stays above text/score.
    let ball_rect = Rectangle::new(
        Point::new(ball_x - ball_r, ball_y - ball_r),
        Size::new((ball_r * 2 + 1) as u32, (ball_r * 2 + 1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(ball_color));
    let _ = ball_rect.draw(disp);
    pong_ball_set_last_pos(Some((ball_x, ball_y)));

    pong_paddle_set_last_angle(Some(angle));
}
