// Minimal CO5300 panel driver (QSPI mode).
// Works with esp-hal (no_std) and embedded-graphics.
//
// Wiring on Waveshare ESP32-S3 Touch AMOLED 1.43” (CO5300):
//   CS  = GPIO9
//   SCK = GPIO10
//   IO0/MOSI = GPIO11
//   IO1/MISO = GPIO12
//   IO2 = GPIO13
//   IO3 = GPIO14
//   RST = GPIO21
//
// Protocol (Standard SPI):
//   Every write begins with 0x02, then one byte CMD, then N data bytes.
//   Example: [0x02, 0x11] -> Sleep Out
//            [0x02, 0x3A, 0x55] -> Pixel Format = 16bpp (RGB565)
// Geometry: panel is 466 x 466 logical pixels (square).
// Datasheet: https://admin.osptek.com/uploads/CO_5300_Datasheet_V0_00_20230328_07edb82936.pdf

use core::fmt;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use embedded_hal::digital::OutputPin;

use esp_hal::{gpio::Output, Blocking};

use embedded_graphics::prelude::IntoStorage;

use esp_hal::spi::master::{Address, Command, DataMode, SpiDmaBus};
// use embedded_hal::delay::DelayNs;

extern crate alloc;
use bytemuck::cast_slice;

// Public constants so the rest of your code can adopt 466×466 easily.
pub const CO5300_WIDTH: u16 = 466;
pub const CO5300_HEIGHT: u16 = 466;
const RAMWR_OPCODE: u8 = 0x2C;
const RAMWRC_OPCODE: u8 = 0x3C;

// Use a small CPU staging buffer per call (HAL will copy it into DMA TX buffer)
const STAGE_BYTES: usize = 4096; // safe on stack; adjust if needed
const DMA_CHUNK_SIZE: usize = 32 * 1023; // max DMA chunk size for ESP32-S3 SPI

// Error type that wraps SPI and GPIO errors.
#[derive(Debug)]
pub enum Co5300Error<SpiE, GpioE> {
    Spi(SpiE),
    Gpio(GpioE),
    OutOfBounds,
}

impl<SpiE: fmt::Debug, GpioE: fmt::Debug> From<SpiE> for Co5300Error<SpiE, GpioE> {
    fn from(e: SpiE) -> Self {
        Self::Spi(e)
    }
}

// A very small CO5300 panel driver speaking the "0x02 + CMD + DATA" SPI framing.
// No D/C pin is used; CS is handled by the `SpiDevice` implementation.
// Implements `DrawTarget<Rgb565>` for convenience (per-pixel path is simple but slow).
pub struct Co5300Display<'fb, RST> {
    pub spi: RawSpiDev<'fb>,
    rst: Option<RST>,
    w: u16,
    h: u16,
    x_off: u16,
    y_off: u16,
    fb: &'fb mut [u16],             // framebuffer storage
    stage: alloc::boxed::Box<[u8]>, // staging buffer for writes
}

impl<'fb, RST> Co5300Display<'fb, RST>
where
    RST: OutputPin,
{
    // Write a BE RGB565 rectangle into the framebuffer only (no flush).
    pub fn write_rect_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        data: &[u8],
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        if data.len() != (w as usize) * (h as usize) * 2 {
            return Err(Co5300Error::OutOfBounds);
        }
        let x0 = x as usize;
        let y0 = y as usize;
        let w_us = w as usize;
        let h_us = h as usize;
        if x0 >= self.w as usize || y0 >= self.h as usize {
            return Err(Co5300Error::OutOfBounds);
        }
        if x0 + w_us > self.w as usize || y0 + h_us > self.h as usize {
            return Err(Co5300Error::OutOfBounds);
        }
        let fbw = self.w as usize;
        let mut src_off = 0usize;
        for row in 0..h_us {
            let dst_base = (y0 + row) * fbw + x0;
            let dst = &mut self.fb[dst_base..dst_base + w_us];
            for px in dst.iter_mut() {
                let b0 = data[src_off];
                let b1 = data[src_off + 1];
                *px = u16::from_be_bytes([b0, b1]).to_be();
                src_off += 2;
            }
        }
        Ok(())
    }

    // Create + init the panel. Call once at startup.
    //
    // * `spi` - an SPI device with CS control (e.g., `embedded_hal_bus::spi::ExclusiveDevice`)
    // * `rst` - optional reset pin (recommended to wire)
    // * `delay` - any `DelayNs` impl (spin delay is fine)
    // * `width`, `height` - normally 466x466 for this AMOLED
    pub fn new(
        spi: RawSpiDev<'fb>,
        rst: Option<RST>,
        delay: &mut impl embedded_hal::delay::DelayNs,
        width: u16,
        height: u16,
        fb: &'fb mut [u16],
    ) -> Result<Self, Co5300Error<(), RST::Error>> {
        // Validate FB size matches WxH (RGB565)
        let expected = (width as usize) * (height as usize);
        if fb.len() != expected {
            return Err(Co5300Error::OutOfBounds);
        }

        // Construct with NO offsets and NO even alignment for now
        let mut this = Self {
            spi,
            rst,
            w: width,
            h: height,
            x_off: 0x0006,
            y_off: 0x0000,
            fb,
            stage: alloc::vec![0u8; STAGE_BYTES].into_boxed_slice(),
        };

        // Hard reset sequence
        if let Some(r) = this.rst.as_mut() {
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(2);
            r.set_low().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(80);
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(200);
        }

        // SW reset + settle
        this.cmd(0x01, &[])?; // SWRESET
        delay.delay_ms(150); // was 120

        // Sleep out + settle
        this.cmd(0x11, &[])?;
        delay.delay_ms(180); // was 150

        // Pixel format + small settle
        this.cmd(0x3A, &[0x55])?;
        delay.delay_ms(2);

        // 0xC4 0x80
        this.cmd(0xC4, &[0x80])?;

        this.cmd(0x13, &[])?; // NORMAL DISPLAY MODE

        // 0x53 0x20 (BCTRL), 1 ms delay
        this.cmd(0x53, &[0x20])?;
        delay.delay_ms(1);

        // 0x63 0xFF (vendor enable), 1 ms delay
        this.cmd(0x63, &[0xFF])?;
        delay.delay_ms(1);

        // 0x51 0x00 (brightness 0), 1 ms delay
        this.cmd(0x51, &[0x00])?;
        delay.delay_ms(1);

        // Display ON + longer settle before any RAMWR
        this.cmd(0x29, &[])?;
        delay.delay_ms(200); // was 80, give panel more time

        // 0x51 0xFF (brightness max)
        this.cmd(0x51, &[0xFF])?;

        // this.cmd(0x38, &[])?;   // Enable QPI / Quad SPI
        // delay.delay_ms(10);

        // // lower brightness
        // this.cmd(0x51, &[0x80])?;

        // Set memory access control (orientation)
        this.cmd(0x36, &[0x00])?;

        // Set full window
        this.cmd(
            0x2A,
            &[
                0x00,
                0x00,
                ((width - 1) >> 8) as u8,
                ((width - 1) & 0xFF) as u8,
            ],
        )?;
        this.cmd(
            0x2B,
            &[
                0x00,
                0x00,
                ((height - 1) >> 8) as u8,
                ((height - 1) & 0xFF) as u8,
            ],
        )?;

        this.fb.fill(0); // clear FB

        Ok(this)
    }

    // Panel width in pixels.
    #[inline]
    pub fn width(&self) -> u16 {
        self.w
    }

    // Panel height in pixels.
    #[inline]
    pub fn height(&self) -> u16 {
        self.h
    }

    // Panel Size
    pub fn size(&self) -> (u16, u16) {
        (self.w, self.h)
    }

    // Raw window set (no even expansion, still applies panel offsets)
    fn set_window_raw(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        // Bounds check
        if x0 > x1 || y0 > y1 || x1 >= self.w || y1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }

        // Apply panel offsets
        let x0p = x0 + self.x_off;
        let x1p = x1 + self.x_off;
        let y0p = y0 + self.y_off;
        let y1p = y1 + self.y_off;

        // Set column and row addresses
        let ca = [
            (x0p >> 8) as u8,
            (x0p & 0xFF) as u8,
            (x1p >> 8) as u8,
            (x1p & 0xFF) as u8,
        ];
        let ra = [
            (y0p >> 8) as u8,
            (y0p & 0xFF) as u8,
            (y1p >> 8) as u8,
            (y1p & 0xFF) as u8,
        ];

        // Send commands
        self.cmd(0x2A, &ca)?;
        self.cmd(0x2B, &ra)?;
        Ok(())
    }

    // QSPI variant: send CASET/RASET using quad instruction/address/data while in QPI.
    fn qspi_set_window_raw(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        if x0 > x1 || y0 > y1 || x1 >= self.w || y1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }

        let x0p = x0 + self.x_off;
        let x1p = x1 + self.x_off;
        let y0p = y0 + self.y_off;
        let y1p = y1 + self.y_off;

        let ca = [
            (x0p >> 8) as u8,
            (x0p & 0xFF) as u8,
            (x1p >> 8) as u8,
            (x1p & 0xFF) as u8,
        ];
        let ra = [
            (y0p >> 8) as u8,
            (y0p & 0xFF) as u8,
            (y1p >> 8) as u8,
            (y1p & 0xFF) as u8,
        ];

        // Quad command writer: opcode 0x02, address on quad, data on quad
        let mut send_cmd_qspi = |cmd: u8, data: &[u8]| -> Result<(), Co5300Error<(), RST::Error>> {
            let instruction = Command::_8Bit(0x02, DataMode::Quad);
            let address = Address::_24Bit((cmd as u32) << 8, DataMode::Quad);
            let _ = self.spi.cs.set_low();
            let res = self
                .spi
                .bus
                .half_duplex_write(DataMode::Quad, instruction, address, 0, data);
            let _ = self.spi.cs.set_high();
            res.map_err(|_| Co5300Error::Spi(()))
        };

        send_cmd_qspi(0x2A, &ca)?;
        send_cmd_qspi(0x2B, &ra)?;
        Ok(())
    }

    // //---- Power control ---- all untested:
    // Quick blank/unblank without sleep
    pub fn display_off(&mut self) -> Result<(), Co5300Error<(), RST::Error>> {
        self.qspi_exit_single();
        let res = self.cmd(0x28, &[]); // DISP OFF
        self.qspi_enter_quad();
        res
    }

    pub fn display_on(
        &mut self,
        delay: &mut impl embedded_hal::delay::DelayNs,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.qspi_exit_single();
        let res = self.cmd(0x29, &[]); // DISP ON
        self.qspi_enter_quad();
        delay.delay_ms(10);
        res
    }

    // Deep sleep control
    pub fn sleep_in(
        &mut self,
        delay: &mut impl embedded_hal::delay::DelayNs,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.qspi_exit_single();
        let res = self.cmd(0x10, &[]); // SLP IN
        self.qspi_enter_quad();
        delay.delay_ms(120);
        res
    }

    pub fn sleep_out(
        &mut self,
        delay: &mut impl embedded_hal::delay::DelayNs,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.qspi_exit_single();
        let res = self.cmd(0x11, &[]); // SLP OUT
        self.qspi_enter_quad();
        delay.delay_ms(120);
        res
    }

    // Convenience wrappers
    pub fn disable(
        &mut self,
        delay: &mut impl embedded_hal::delay::DelayNs,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.display_off()?;
        self.sleep_in(delay)?;
        Ok(())
    }

    pub fn enable(
        &mut self,
        delay: &mut impl embedded_hal::delay::DelayNs,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.sleep_out(delay)?;
        // Re-assert format/orientation if needed
        self.qspi_exit_single();
        self.cmd(0x3A, &[0x55])?; // RGB565
        self.cmd(0x36, &[0x00])?; // MADCTL
                                  // Optionally restore brightness
        self.set_brightness(0xFF)?;
        self.qspi_enter_quad();

        self.display_on(delay)?;
        Ok(())
    }

    // adjustable brightness (0-255)
    pub fn set_brightness(&mut self, bright: u8) -> Result<(), Co5300Error<(), RST::Error>> {
        // exit qspi if needed
        self.qspi_exit_single();
        let res = self.cmd(0x51, &[bright]);
        self.qspi_enter_quad();
        res
    }

    // Flush an FB rectangle, forcing even start/end (2x2 tiles), using raw window, important for embedded-graphics integration.
    fn flush_fb_rect_even(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        // Bounds check
        if x0 > x1 || y0 > y1 || x0 >= self.w || y0 >= self.h {
            return Ok(());
        }

        // Expand to even boundaries
        let ax0 = x0 & !1;
        let ay0 = y0 & !1;
        let ax1 = (x1 | 1).min(self.w - 1);
        let ay1 = (y1 | 1).min(self.h - 1);
        let ew = (ax1 - ax0 + 1) as usize;

        // Use quad window and quad payload streaming
        self.qspi_set_window_raw(ax0, ay0, ax1, ay1)?;

        let fbw = self.w as usize;
        let instruction = Command::_8Bit(0x32, DataMode::Quad);
        let mut current_cmd = RAMWR_OPCODE;
        let address_mode = DataMode::Quad;
        let data_mode = DataMode::Quad;
        let bus: &mut SpiDmaBus<'fb, Blocking> = &mut self.spi.bus;

        // Use staging buffer directly; no realloc per call
        let stage = &mut self.stage;
        let mut filled = 0usize;

        for y in ay0..=ay1 {
            // get row slice from FB
            let row_base = (y as usize) * fbw + (ax0 as usize);
            let row = &self.fb[row_base..row_base + ew];
            let row_bytes = cast_slice(row);
            let mut off = 0usize;

            // stream row bytes into staging buffer and send when full
            while off < row_bytes.len() {
                let space = stage.len().saturating_sub(filled);
                let take = core::cmp::min(space, row_bytes.len() - off);
                stage[filled..filled + take].copy_from_slice(&row_bytes[off..off + take]);
                filled += take;
                off += take;

                if filled == stage.len() {
                    // send current chunk over quad
                    let ad: u32 = (current_cmd as u32) << 8;
                    let address = Address::_24Bit(ad, address_mode);
                    let _ = self.spi.cs.set_low();
                    let res =
                        bus.half_duplex_write(data_mode, instruction, address, 0, &stage[..filled]);
                    let _ = self.spi.cs.set_high();
                    res.map_err(|_| Co5300Error::Spi(()))?;
                    current_cmd = RAMWRC_OPCODE;
                    filled = 0;
                }
            }
        }

        // send any remaining data
        if filled > 0 {
            let ad: u32 = (current_cmd as u32) << 8;
            let address = Address::_24Bit(ad, address_mode);
            let _ = self.spi.cs.set_low();
            let res = bus.half_duplex_write(data_mode, instruction, address, 0, &stage[..filled]);
            let _ = self.spi.cs.set_high();
            res.map_err(|_| Co5300Error::Spi(()))?;
        }

        Ok(())
    }

    // Public wrapper to flush an FB rectangle.
    pub fn flush_rect_even(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.flush_fb_rect_even(x0, y0, x1, y1)
    }

    // Draw a line directly into the framebuffer (no flush). Returns the drawn bounding box. Used for certain specific graphics.
    pub fn draw_line_fb(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgb565,
        stroke: u8,
    ) -> Option<(u16, u16, u16, u16)> {
        let w = self.w as i32;
        let h = self.h as i32;
        if w == 0 || h == 0 {
            return None;
        }
        let mut x0 = x0;
        let mut y0 = y0;
        let x1 = x1;
        let y1 = y1;

        // Bresenham with clipping by skipping off-screen pixels.
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        let mut minx = i32::MAX;
        let mut miny = i32::MAX;
        let mut maxx = i32::MIN;
        let mut maxy = i32::MIN;

        let cbe = color.into_storage().to_be();
        let stroke_span = stroke.max(1) as i32;
        let half = stroke_span / 2;

        loop {
            if x0 >= -half && x0 < w + half && y0 >= -half && y0 < h + half {
                let start_x = (x0 - half).max(0);
                let start_y = (y0 - half).max(0);
                let end_x = (x0 + (stroke_span - half - 1)).min(w - 1);
                let end_y = (y0 + (stroke_span - half - 1)).min(h - 1);
                for yy in start_y..=end_y {
                    let base = (yy as usize) * (self.w as usize);
                    for xx in start_x..=end_x {
                        self.fb[base + xx as usize] = cbe;
                    }
                }
                minx = minx.min(start_x);
                miny = miny.min(start_y);
                maxx = maxx.max(end_x);
                maxy = maxy.max(end_y);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }

        if minx == i32::MAX {
            None
        } else {
            Some((minx as u16, miny as u16, maxx as u16, maxy as u16))
        }
    }

    // Fill a rectangle in the framebuffer with a solid color (no flush), used for certain specific graphics.
    pub fn fill_rect_fb(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb565) {
        let w = self.w as i32;
        let h = self.h as i32;
        if w == 0 || h == 0 {
            return;
        }
        let (mut x0, mut x1) = (x0.min(x1), x0.max(x1));
        let (mut y0, mut y1) = (y0.min(y1), y0.max(y1));
        x0 = x0.max(0);
        y0 = y0.max(0);
        x1 = x1.min(w - 1);
        y1 = y1.min(h - 1);
        if x0 > x1 || y0 > y1 {
            return;
        }
        let fbw = self.w as usize;
        let cbe = color.into_storage().to_be();
        for yy in y0..=y1 {
            let base = (yy as usize) * fbw + (x0 as usize);
            let width = (x1 - x0 + 1) as usize;
            let row = &mut self.fb[base..base + width];
            for px in row.iter_mut() {
                *px = cbe;
            }
        }
    }

    // Convenience: fill a rectangle with a solid color, using staging buffer.
    pub fn fill_rect_solid(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.fill_rect_solid_opt(x, y, w, h, color, true)
    }

    // Same as `fill_rect_solid` but optionally skips framebuffer mirroring for speed.
    pub fn fill_rect_solid_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.fill_rect_solid_opt(x, y, w, h, color, false)
    }

    // Core implementation of solid fill with optional FB update, main purpose is for speed, e.g., clearing.
    fn fill_rect_solid_opt(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
        update_fb: bool,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        if w == 0 || h == 0 {
            return Ok(());
        }

        // overflow-safe bounds
        let (pw, ph) = (self.w as u32, self.h as u32);
        let (x0, y0, w32, h32) = (x as u32, y as u32, w as u32, h as u32);
        if x0 >= pw || y0 >= ph {
            return Err(Co5300Error::OutOfBounds);
        }
        if x0.checked_add(w32).unwrap_or(u32::MAX) > pw
            || y0.checked_add(h32).unwrap_or(u32::MAX) > ph
        {
            return Err(Co5300Error::OutOfBounds);
        }
        let (x1, y1) = ((x0 + w32 - 1) as u16, (y0 + h32 - 1) as u16);

        // Set window
        self.qspi_set_window_raw(x, y, x1, y1)?;

        // Prepare staging buffer with color pattern
        let stage = &mut self.stage;

        // Prepare BE pattern once
        let c = color.into_storage().to_be_bytes();
        for i in (0..stage.len()).step_by(2) {
            stage[i] = c[0];
            if i + 1 < stage.len() {
                stage[i + 1] = c[1];
            }
        }

        // Send in chunks
        let mut remaining = (w as usize) * (h as usize) * 2;
        let instruction = Command::_8Bit(0x32, DataMode::Quad);
        let data_mode = DataMode::Quad;
        let address_mode = DataMode::Quad;
        let bus: &mut SpiDmaBus<'fb, Blocking> = &mut self.spi.bus;
        let mut current_cmd = RAMWR_OPCODE;

        // Stream full chunks
        while remaining > 0 {
            let take = core::cmp::min(stage.len(), remaining);
            let ad: u32 = (current_cmd as u32) << 8;
            let address = Address::_24Bit(ad, address_mode);
            let _ = self.spi.cs.set_low();
            let res = bus.half_duplex_write(data_mode, instruction, address, 0, &stage[..take]);
            let _ = self.spi.cs.set_high();
            res.map_err(|_| Co5300Error::Spi(()))?;
            remaining -= take;
            current_cmd = RAMWRC_OPCODE;
        }

        // Mirror into FB
        if update_fb {
            let fbw = self.w as usize;
            let row_w = w as usize;
            let col_start = x as usize;
            let row_start = y as usize;
            let color16 = color.into_storage().to_be();
            for ry in 0..(h as usize) {
                let base = (row_start + ry) * fbw + col_start;
                let dst = &mut self.fb[base..base + row_w];
                for px in dst.iter_mut() {
                    *px = color16;
                }
            }
        }
        Ok(())
    }

    // Chunked rect blit from BE bytes; send slices directly using quad and mirror into FB.
    pub fn blit_rect_be_fast(
        &mut self,
        x0: u16,
        y0: u16,
        w: u16,
        h: u16,
        data: &[u8],
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.blit_rect_be_fast_opt(x0, y0, w, h, data, true)
    }

    // Same as `blit_rect_be_fast` but optionally skips framebuffer mirroring for speed.
    pub fn blit_rect_be_fast_no_fb(
        &mut self,
        x0: u16,
        y0: u16,
        w: u16,
        h: u16,
        data: &[u8],
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        self.blit_rect_be_fast_opt(x0, y0, w, h, data, false)
    }

    // Core implementation of blit with optional FB update, main purpose is for loading images
    fn blit_rect_be_fast_opt(
        &mut self,
        x0: u16,
        y0: u16,
        w: u16,
        h: u16,
        data: &[u8],
        update_fb: bool,
    ) -> Result<(), Co5300Error<(), RST::Error>> {
        // early out
        if w == 0 || h == 0 {
            return Ok(());
        }

        // overflow-safe bounds
        let (pw, ph) = (self.w as u32, self.h as u32);
        let (x32, y32, w32, h32) = (x0 as u32, y0 as u32, w as u32, h as u32);
        if x32 >= pw || y32 >= ph {
            return Err(Co5300Error::OutOfBounds);
        }
        if x32.checked_add(w32).unwrap_or(u32::MAX) > pw
            || y32.checked_add(h32).unwrap_or(u32::MAX) > ph
        {
            return Err(Co5300Error::OutOfBounds);
        }

        // calculate bottom-right
        let (x1, y1) = ((x32 + w32 - 1) as u16, (y32 + h32 - 1) as u16);

        // validate data length
        let expected = (w as usize) * (h as usize) * 2;

        // each pixel is 2 bytes (RGB565 BE)
        if data.len() != expected {
            return Err(Co5300Error::OutOfBounds);
        }

        // Set window
        self.qspi_set_window_raw(x0, y0, x1, y1)?;

        // Stream in chunks
        let mut off = 0usize;
        let mut current_cmd = RAMWR_OPCODE;
        let instruction = Command::_8Bit(0x32, DataMode::Quad);
        let address_mode = DataMode::Quad;
        let data_mode = DataMode::Quad;
        let bus: &mut SpiDmaBus<'fb, Blocking> = &mut self.spi.bus;

        // Stream full chunks
        while off < data.len() {
            let take = core::cmp::min(DMA_CHUNK_SIZE, data.len() - off);
            let ad: u32 = (current_cmd as u32) << 8;
            let address = Address::_24Bit(ad, address_mode);
            let chunk = &data[off..off + take];
            let _ = self.spi.cs.set_low();
            let res = bus.half_duplex_write(data_mode, instruction, address, 0, chunk);
            let _ = self.spi.cs.set_high();
            res.map_err(|_| Co5300Error::Spi(()))?;
            off += take;
            current_cmd = RAMWRC_OPCODE;
        }

        // Update FB (convert BE bytes to native u16)
        if update_fb {
            let fbw = self.w as usize;
            let mut si = 0usize;
            for ry in 0..(h as usize) {
                let base = (y0 as usize + ry) * fbw + (x0 as usize);
                let row = &mut self.fb[base..base + (w as usize)];
                for px in row.iter_mut() {
                    let hi = data[si];
                    let lo = data[si + 1];
                    *px = u16::from_be_bytes([hi, lo]).to_be();
                    si += 2;
                }
            }
        }
        Ok(())
    }

    // ---- Low-level helpers ----
    // Low-level command send (with data)
    #[inline(always)]
    fn cmd(&mut self, cmd: u8, data: &[u8]) -> Result<(), Co5300Error<(), RST::Error>> {
        let _ = self.spi.cs.set_low();
        let res = self.spi.bus.half_duplex_write(
            DataMode::Single,
            Command::_8Bit(0x02, DataMode::Single),
            Address::_24Bit((cmd as u32) << 8, DataMode::Single),
            0,
            data,
        );
        let _ = self.spi.cs.set_high();
        res.map_err(|_| Co5300Error::Spi(()))
    }

    // Send a bare QSPI mode-change instruction (0x38 enter, 0x3B enter dual, 0xFF exit).
    // Must be sent in the *current* bus width (we enter from single, so use 1-wire).
    fn qspi_send_mode_instr(&mut self, instr: u8, mode: DataMode) {
        let command = Command::_8Bit(instr as u16, mode);
        let bus: &mut SpiDmaBus<'fb, Blocking> = &mut self.spi.bus;
        let _ = self.spi.cs.set_low();
        let _ = bus.half_duplex_write(mode, command, Address::None, 0, &[]);
        let _ = self.spi.cs.set_high();
    }

    // Enter quad-data mode (enable QPI, per CO5300 table: 0x38).
    fn qspi_enter_quad(&mut self) {
        self.qspi_send_mode_instr(0x38, DataMode::Single);
    }

    // go back to 1-wire SPI (0xFF).
    fn qspi_exit_single(&mut self) {
        self.qspi_send_mode_instr(0xFF, DataMode::Quad);
    }
}

// -------------------- embedded-graphics integration --------------------
impl<'fb, RST> OriginDimensions for Co5300Display<'fb, RST>
where
    RST: OutputPin,
{
    // Return the size of the display.
    fn size(&self) -> Size {
        Size::new(self.w as u32, self.h as u32)
    }
}

impl<'fb, RST> embedded_graphics::draw_target::DrawTarget for Co5300Display<'fb, RST>
where
    RST: embedded_hal::digital::OutputPin,
{
    type Color = embedded_graphics::pixelcolor::Rgb565;
    type Error = core::convert::Infallible;

    // SLOW PATH: per-pixel drawing (inefficient, but simple)
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Rgb565>>,
    {
        use embedded_graphics::Pixel;

        // Track dirty rectangle
        let mut any = false;
        let mut minx = self.w;
        let mut miny = self.h;
        let mut maxx: u16 = 0;
        let mut maxy: u16 = 0;

        // Update FB and dirty rect
        for Pixel(p, c) in pixels {
            if p.x < 0 || p.y < 0 {
                continue;
            }
            let (x, y) = (p.x as u16, p.y as u16);
            if x >= self.w || y >= self.h {
                continue;
            }
            self.fb[(y as usize) * (self.w as usize) + (x as usize)] = c.into_storage().to_be();

            if !any {
                any = true;
                minx = x;
                maxx = x;
                miny = y;
                maxy = y;
            } else {
                if x < minx {
                    minx = x;
                }
                if y < miny {
                    miny = y;
                }
                if x > maxx {
                    maxx = x;
                }
                if y > maxy {
                    maxy = y;
                }
            }
        }

        // Flush dirty rectangle if any
        if any {
            let _ = self.flush_fb_rect_even(minx, miny, maxx, maxy);
        }
        Ok(())
    }

    // FAST PATH: row streaming for images and large fills
    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        use embedded_graphics::prelude::*;

        // Clip to panel
        let bounds = Rectangle::new(Point::new(0, 0), Size::new(self.w as u32, self.h as u32));
        let inter = area.intersection(&bounds);
        if inter.size.width == 0 || inter.size.height == 0 {
            // Drain iterator to keep semantics
            let mut it = colors.into_iter();
            let total = (area.size.width as usize) * (area.size.height as usize);
            for _ in 0..total {
                let _ = it.next();
            }
            return Ok(());
        }

        // Precompute skips and takes
        let area_w = area.size.width as usize;
        let area_h = area.size.height as usize;

        // Intersection coords
        let x0 = inter.top_left.x as u16;
        let y0 = inter.top_left.y as u16;
        let take = inter.size.width as usize;
        let inter_h = inter.size.height as usize;

        // Skips
        let left_skip = (inter.top_left.x - area.top_left.x).max(0) as usize;
        let right_skip = area_w.saturating_sub(left_skip + take);
        let top_skip = (inter.top_left.y - area.top_left.y).max(0) as usize;

        // Consume iterator once, write directly into FB for visible rectangle
        let fbw = self.w as usize;
        let mut it = colors.into_iter();

        // Skip rows above intersection
        for _ in 0..top_skip {
            for _ in 0..area_w {
                let _ = it.next();
            }
        }

        // Rows in intersection
        for ry in 0..inter_h {
            // Skip left columns
            for _ in 0..left_skip {
                let _ = it.next();
            }

            // Write visible span into FB
            let dst_row = (y0 as usize + ry) * fbw;
            let dst_off = dst_row + (x0 as usize);
            for cx in 0..take {
                if let Some(c) = it.next() {
                    self.fb[dst_off + cx] = c.into_storage().to_be();
                }
            }

            // Skip right columns
            for _ in 0..right_skip {
                let _ = it.next();
            }
        }

        // Drain rows below to preserve iterator semantics
        let rows_below = area_h.saturating_sub(top_skip + inter_h);
        for _ in 0..rows_below {
            for _ in 0..area_w {
                let _ = it.next();
            }
        }

        // One flush from FB (handles even-alignment + single RAMWR)
        let x1 = x0 + (take as u16) - 1;
        let y1 = y0 + (inter_h as u16) - 1;
        let _ = self.flush_fb_rect_even(x0, y0, x1, y1);

        Ok(())
    }

    fn clear(&mut self, color: embedded_graphics::pixelcolor::Rgb565) -> Result<(), Self::Error> {
        // Use fast fill rect, framebuffer will be updated too, call fill_rect_solid_no_fb to skip FB update
        let _ = self.fill_rect_solid(0, 0, self.w, self.h, color);
        Ok(())
    }
}

// Convenience builder that picks common defaults and returns the concrete type.
// Returning the concrete type lets display.rs use `impl Trait` to erase it later.
pub fn new_with_defaults<'fb, RST>(
    spi: RawSpiDev<'fb>,
    rst: Option<RST>,
    delay: &mut impl embedded_hal::delay::DelayNs,
    fb: &'fb mut [u16],
) -> Result<Co5300Display<'fb, RST>, Co5300Error<(), RST::Error>>
where
    RST: embedded_hal::digital::OutputPin,
{
    let mut display = Co5300Display::new(spi, rst, delay, CO5300_WIDTH, CO5300_HEIGHT, fb)?;
    display.set_window_raw(0, 0, CO5300_WIDTH - 1, CO5300_HEIGHT - 1)?;
    // Enter QPI once; we will stay in quad for pixel data and revert only if caller asks.
    display.qspi_enter_quad();
    Ok(display)
}

// Raw SPI container: manual CS + bus so we can wrap half_duplex writes ourselves.
pub struct RawSpiDev<'a> {
    pub bus: SpiDmaBus<'a, Blocking>,
    pub cs: Output<'a>,
}

// Keep this type alias in sync with display.rs
pub type DisplayType<'a> = Co5300Display<'a, Output<'a>>;
