
// Minimal CO5300 panel driver (Standard SPI mode, no D/C pin).
// Works with esp-hal (no_std) and embedded-graphics.
//
// Wiring on Waveshare ESP32-S3 Touch AMOLED 1.43” (CO5300):
//   CS  = GPIO9
//   SCK = GPIO10
//   IO0/MOSI = GPIO11
//   (IO1..IO3 unused in Standard SPI mode)
//   RST = GPIO21
//
// Protocol (Standard SPI):
//   Every write begins with 0x02, then one byte CMD, then N data bytes.
//   Example: [0x02, 0x11] -> Sleep Out
//            [0x02, 0x3A, 0x55] -> Pixel Format = 16bpp (RGB565)
// Geometry: panel is 466 x 466 logical pixels (square).

use core::{fmt, mem};

use embedded_graphics::{
    primitives::Rectangle,
    pixelcolor::Rgb565,
    prelude::*,
};
use embedded_hal::{
    digital::OutputPin,
    spi::SpiDevice,
};

use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};

use esp_hal::{
    Blocking,
    gpio::Output,
    spi::master::Spi,
};

use embedded_hal::spi::Operation;
use embedded_graphics::prelude::IntoStorage;

extern crate alloc;

// Public constants so the rest of your code can adopt 466×466 easily.
pub const CO5300_WIDTH: u16 = 466;
pub const CO5300_HEIGHT: u16 = 466;
const RAMWR_OPCODE: u8 = 0x2C;
const RAMWRC_OPCODE: u8 = 0x3C;

// Use a small CPU staging buffer per call (HAL will copy it into DMA TX buffer)
const STAGE_BYTES: usize = 2048; // safe on stack; adjust if needed
const DMA_CHUNK_SIZE: usize = 32*1023; // 32*1023=32768 minus some overhead

/// Low-level command send helper, for debugging
// #[esp_hal::ram] // run from IRAM
pub fn ramwr_stream<SD: SpiDevice<u8>>(
    spi: &mut SD,
    chunks: &[&[u8]],
) -> Result<(), SD::Error> {
    use embedded_hal::spi::Operation;

    if chunks.is_empty() {
        return Ok(());
    }

    let hdr: [u8; 4] = [0x02, 0x00, RAMWR_OPCODE, 0x00];

    // Header + many data chunks in one CS-asserted transaction
    let mut ops: heapless::Vec<Operation<'_, u8>, 512> = heapless::Vec::new();
    ops.push(Operation::Write(&hdr)).ok();

    for &c in chunks {
        if !c.is_empty() {
            ops.push(Operation::Write(c)).ok();
        }
    }

    spi.transaction(&mut ops)
}

/// Error type that wraps SPI and GPIO errors.
#[derive(Debug)]
pub enum Co5300Error<SpiE, GpioE> {
    Spi(SpiE),
    Gpio(GpioE),
    OutOfBounds,
}

impl<SpiE: fmt::Debug, GpioE: fmt::Debug> From<SpiE> for Co5300Error<SpiE, GpioE> {
    fn from(e: SpiE) -> Self { Self::Spi(e) }
}

// A very small CO5300 panel driver speaking the "0x02 + CMD + DATA" SPI framing.
// No D/C pin is used; CS is handled by the `SpiDevice` implementation.
// Implements `DrawTarget<Rgb565>` for convenience (per-pixel path is simple but slow).
pub struct Co5300Display<'fb,SPI, RST> {
    pub spi: SPI,
    rst: Option<RST>,
    w: u16,
    h: u16,
    x_off: u16,
    y_off: u16,
    fb: &'fb mut [u16], // framebuffer storage
    stage: alloc::boxed::Box<[u8]>, // staging buffer for writes
}


impl<'fb, SPI, RST> Co5300Display<'fb, SPI, RST>
where
    // embedded-hal 1.0 `SpiDevice<u8>` so we can do atomic CS-asserted transfers.
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    // Create + init the panel. Call once at startup.
    //
    // * `spi` - an SPI device with CS control (e.g., `embedded_hal_bus::spi::ExclusiveDevice`)
    // * `rst` - optional reset pin (recommended to wire)
    // * `delay` - any `DelayNs` impl (spin delay is fine)
    // * `width`, `height` - normally 466x466 for this AMOLED
    pub fn new(
        spi: SPI,
        rst: Option<RST>,
        delay: &mut impl embedded_hal::delay::DelayNs,
        width: u16,
        height: u16,
        fb: &'fb mut [u16],
    ) -> Result<Self, Co5300Error<SPI::Error, RST::Error>> {

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
        delay.delay_ms(150);  // was 120

        // Sleep out + settle
        this.cmd(0x11, &[])?;
        delay.delay_ms(180);  // was 150

        // Pixel format + small settle
        this.cmd(0x3A, &[0x55])?;
        delay.delay_ms(2);

        // 0xC4 0x80
        this.cmd(0xC4, &[0x80])?;

        this.cmd(0x13, &[])?;          // NORMAL DISPLAY MODE

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
        delay.delay_ms(200);  // was 80, give panel more time

        // // 0x51 0xFF (brightness max)
        // this.cmd(0x51, &[0xFF])?;

        // lower brightness
        this.cmd(0x51, &[0x80])?;

        // Set memory access control (orientation)
        this.cmd(0x36, &[0x00])?; 

        // Set full window
        this.cmd(0x2A, &[0x00, 0x00, ((width-1)>>8) as u8, ((width-1)&0xFF) as u8])?;
        this.cmd(0x2B, &[0x00, 0x00, ((height-1)>>8) as u8, ((height-1)&0xFF) as u8])?;
        
        this.fb.fill(0); // clear FB
        
        Ok(this)
    }

    // Expose the underlying SPI device to allow changing frequency later.
    // WARNING: Only do this when no transfer is active.
    pub fn spi_mut(&mut self) -> &mut SPI {
        &mut self.spi
    }

    // Panel width in pixels.
    #[inline]
    pub fn width(&self) -> u16 { self.w }

    // Panel height in pixels.
    #[inline]
    pub fn height(&self) -> u16 { self.h }

    // Panel Size
    pub fn size(&self) -> (u16, u16) { (self.w, self.h) }

    // Raw window set (no even expansion, still applies panel offsets)
    // #[esp_hal::ram] // run from IRAM
    fn set_window_raw(
        &mut self,
        x0: u16, y0: u16,
        x1: u16, y1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {

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
        let ca = [(x0p >> 8) as u8, (x0p & 0xFF) as u8, (x1p >> 8) as u8, (x1p & 0xFF) as u8];
        let ra = [(y0p >> 8) as u8, (y0p & 0xFF) as u8, (y1p >> 8) as u8, (y1p & 0xFF) as u8];

        // Send commands
        self.cmd(0x2A, &ca)?;
        self.cmd(0x2B, &ra)?;
        Ok(())
    }


    // TODO LETS TEST THESE, also I want brightness control
    // //---- Power control ---- all untested:
    // // Quick blank/unblank without sleep
    // pub fn display_off(&mut self) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
    //     self.cmd(0x28, &[])?; // DISP OFF
    //     Ok(())
    // }

    // pub fn display_on(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x29, &[])?; // DISP ON
    //     delay.delay_ms(10);   // small settle before first RAMWR
    //     Ok(())
    // }

    // // Deep sleep control
    // pub fn sleep_in(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x10, &[])?; // SLP IN
    //     delay.delay_ms(120);
    //     Ok(())
    // }

    // pub fn sleep_out(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x11, &[])?; // SLP OUT
    //     delay.delay_ms(120);
    //     Ok(())
    // }

    //     // Convenience: full disable (blank + sleep)
    // pub fn disable(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.display_off()?;
    //     self.sleep_in(delay)?;
    //     Ok(())
    // }

    // // Convenience: full enable (wake + on + re-assert opts if needed)
    // pub fn enable(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.sleep_out(delay)?;

    //     // Some panels lose format/orientation in sleep; re-assert if needed
    //     self.cmd(0x3A, &[0x55])?; // RGB565
    //     self.cmd(0x36, &[0x00])?; // MADCTL (adjust if you rotate)

    //     self.display_on(delay)?;
    //     // Optionally restore brightness
    //     // self.cmd(0x51, &[0xFF])?;
    //     Ok(())
    // }


    // Write a list of pixel rows (each row is &[u8]) in one RAMWR transaction.
    // #[esp_hal::ram] // run from IRAM
    pub fn write_pixels_rows(&mut self, rows: &[&[u8]])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        // Send RAMWR + pixel data
        ramwr_stream(&mut self.spi, rows).map_err(Co5300Error::Spi)
    }


    // temporary for prototyping
    pub fn write_2x2(
        &mut self,
        x: u16,
        y: u16,
        color_1: Rgb565,
        color_2: Rgb565,
        color_3: Rgb565,
        color_4: Rgb565,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x >= self.w || y >= self.h || x + 1 >= self.w || y + 1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }
        // Align to even x (panel quirk-friendly)
        let x0 = x & !1;
        let y0 = y & !1;

        if x0 + 1 >= self.w || y0 + 1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }

        self.set_window_raw(x0, y0, x0 + 1, y0 + 1)?;

        let a = color_1.into_storage().to_be_bytes();
        let b = color_2.into_storage().to_be_bytes();
        let c = color_3.into_storage().to_be_bytes();
        let d = color_4.into_storage().to_be_bytes();

        let row0 = [a[0], a[1], b[0], b[1]];
        let row1 = [c[0], c[1], d[0], d[1]];

        let rows: [&[u8]; 2] = [&row0, &row1];
        self.write_pixels_rows(&rows)
    }

    #[inline(always)]
    fn write_ram_chunk(
        &mut self,
        first_flag: &mut bool,
        chunk: &[u8],
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        let cmd = if *first_flag { RAMWR_OPCODE } else { RAMWRC_OPCODE };
        *first_flag = false;
        let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
        self.spi
            .transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(chunk),
            ])
            .map_err(Co5300Error::Spi)
    }


    // Flush an FB rectangle, forcing even start/end (2x2 tiles), using raw window.
    fn flush_fb_rect_even(
        &mut self,
        x0: u16, y0: u16, x1: u16, y1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x0 > x1 || y0 > y1 || x0 >= self.w || y0 >= self.h {
            return Ok(());
        }

        let ax0 = x0 & !1;
        let ay0 = y0 & !1;
        let ax1 = (x1 | 1).min(self.w - 1);
        let ay1 = (y1 | 1).min(self.h - 1);
        let ew = (ax1 - ax0 + 1) as usize;

        self.set_window_raw(ax0, ay0, ax1, ay1)?;

        let fbw = self.w as usize;
        let mut first = true;

        // Move stage out, use it locally, then restore to self at end
        let mut stage = mem::replace(&mut self.stage, alloc::vec![].into_boxed_slice());
        let mut filled = 0usize;

        for y in ay0..=ay1 {
            let row_base = (y as usize) * fbw + (ax0 as usize);
            for x in 0..ew {
                if filled + 2 > stage.len() {
                    self.write_ram_chunk(&mut first, &stage[..filled])?;
                    filled = 0;
                }
                let be = self.fb[row_base + x].to_be_bytes();
                stage[filled] = be[0];
                stage[filled + 1] = be[1];
                filled += 2;
            }
        }
        self.write_ram_chunk(&mut first, &stage[..filled])?;

        // Restore stage buffer to self
        self.stage = stage;
        Ok(())
    }
    
    // Convenience: fill a rectangle with a solid color, using staging buffer.
    pub fn fill_rect_solid(
        &mut self, x: u16, y: u16, w: u16, h: u16, color: Rgb565,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if w == 0 || h == 0 { return Ok(()); }

        // overflow-safe bounds
        let (pw, ph) = (self.w as u32, self.h as u32);
        let (x0, y0, w32, h32) = (x as u32, y as u32, w as u32, h as u32);
        if x0 >= pw || y0 >= ph { return Err(Co5300Error::OutOfBounds); }
        if x0.checked_add(w32).unwrap_or(u32::MAX) > pw ||
           y0.checked_add(h32).unwrap_or(u32::MAX) > ph {
            return Err(Co5300Error::OutOfBounds);
        }
        let (x1, y1) = ((x0 + w32 - 1) as u16, (y0 + h32 - 1) as u16);

        self.set_window_raw(x, y, x1, y1)?;

        // Move stage out, fill pattern, send in chunks, then restore
        let mut stage = mem::replace(&mut self.stage, alloc::vec![].into_boxed_slice());

        // Prepare BE pattern once
        let c = color.into_storage().to_be_bytes();
        for i in (0..stage.len()).step_by(2) {
            stage[i] = c[0];
            if i + 1 < stage.len() { stage[i + 1] = c[1]; }
        }

        let mut remaining = (w as usize) * (h as usize) * 2;
        let mut first = true;
        while remaining > 0 {
            let take = core::cmp::min(stage.len(), remaining);
            let hdr: [u8; 4] = [0x02, 0x00, if first { RAMWR_OPCODE } else { RAMWRC_OPCODE }, 0x00];
            first = false;
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(&stage[..take]),
            ]).map_err(Co5300Error::Spi)?;
            remaining -= take;
        }

        // Restore stage buffer
        self.stage = stage;

        // Mirror into FB
        let fbw = self.w as usize;
        let row_w = w as usize;
        let col_start = x as usize;
        let row_start = y as usize;
        let color16 = color.into_storage();
        for ry in 0..(h as usize) {
            let base = (row_start + ry) * fbw + col_start;
            let dst = &mut self.fb[base..base + row_w];
            for px in dst.iter_mut() { *px = color16; }
        }
        Ok(())
    }

    // Chunked rect blit from BE bytes; send slices directly (HAL copies to its DMA buffer).
    pub fn blit_rect_be_fast(
        &mut self, 
        x0: u16, 
        y0: u16, 
        w: u16, 
        h: u16, 
        data: &[u8],
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if w == 0 || h == 0 { return Ok(()); }

        // overflow-safe bounds
        let (pw, ph) = (self.w as u32, self.h as u32);
        let (x32, y32, w32, h32) = (x0 as u32, y0 as u32, w as u32, h as u32);
        if x32 >= pw || y32 >= ph { return Err(Co5300Error::OutOfBounds); }
        if x32.checked_add(w32).unwrap_or(u32::MAX) > pw ||
           y32.checked_add(h32).unwrap_or(u32::MAX) > ph {
            return Err(Co5300Error::OutOfBounds);
        }
        let (x1, y1) = ((x32 + w32 - 1) as u16, (y32 + h32 - 1) as u16);

        let expected = (w as usize) * (h as usize) * 2;
        if data.len() != expected { return Err(Co5300Error::OutOfBounds); }

        self.set_window_raw(x0, y0, x1, y1)?;

        // manually chunk to respect the DMA buffer size.
        let mut off = 0usize;
        let mut first = true;
        while off < data.len() {
            // Use our new large chunk size
            let take = core::cmp::min(DMA_CHUNK_SIZE, data.len() - off);
            let cmd = if first { RAMWR_OPCODE } else { RAMWRC_OPCODE };
            first = false;
            let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
            let chunk = &data[off..off + take];
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(chunk),
            ]).map_err(Co5300Error::Spi)?;
            off += take;
        }

        // Update FB (convert BE bytes to native u16)
        let fbw = self.w as usize;
        let mut si = 0usize;
        for ry in 0..(h as usize) {
            let base = (y0 as usize + ry) * fbw + (x0 as usize);
            let row = &mut self.fb[base..base + (w as usize)];
            for px in row.iter_mut() {
                let hi = data[si]; let lo = data[si + 1];
                *px = u16::from_be_bytes([hi, lo]);
                si += 2;
            }
        }
        Ok(())
    }


    // Full-frame blit from BE bytes, using direct slices.
    pub fn blit_full_frame_be_bounced(&mut self, data: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        let needed = (self.w as usize) * (self.h as usize) * 2;
        if data.len() != needed { return Err(Co5300Error::OutOfBounds); }

        self.set_window_raw(0, 0, self.w - 1, self.h - 1)?;

        // manually chunk to respect the DMA buffer size.
        let mut off = 0usize;
        let mut first = true;
        while off < data.len() {
            // Use our new large chunk size
            let take = core::cmp::min(DMA_CHUNK_SIZE, data.len() - off);
            let cmd = if first { RAMWR_OPCODE } else { RAMWRC_OPCODE };
            first = false;
            let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
            let chunk = &data[off..off + take];
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(chunk),
            ]).map_err(Co5300Error::Spi)?;
            off += take;
        }
        // ---- END OF CHANGE ----

        // Update FB
        let mut si = 0usize;
        for px in self.fb.iter_mut() {
            let hi = data[si]; let lo = data[si + 1];
            *px = u16::from_be_bytes([hi, lo]);
            si += 2;
        }
        Ok(())
    }

    // ---- Low-level helpers ----
    #[inline(always)]
    fn cmd(&mut self, cmd: u8, data: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
        if data.is_empty() {
            self.spi.write(&hdr).map_err(|e| {
                Co5300Error::Spi(e)
            })
        } else {
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(data),
            ]).map_err(|e| {
                Co5300Error::Spi(e)
            })
        }
    }
}

// -------------------- embedded-graphics integration --------------------
impl<'fb, SPI, RST> OriginDimensions for Co5300Display<'fb, SPI, RST>
where
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(self.w as u32, self.h as u32)
    }
}

impl<'fb, SPI, RST> embedded_graphics::draw_target::DrawTarget for Co5300Display<'fb, SPI, RST>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    type Color = embedded_graphics::pixelcolor::Rgb565;
    type Error = core::convert::Infallible;

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
            if p.x < 0 || p.y < 0 { continue; }
            let (x, y) = (p.x as u16, p.y as u16);
            if x >= self.w || y >= self.h { continue; }
            self.fb[(y as usize) * (self.w as usize) + (x as usize)] = c.into_storage();

            if !any {
                any = true;
                minx = x; maxx = x;
                miny = y; maxy = y;
            } else {
                if x < minx { minx = x; }
                if y < miny { miny = y; }
                if x > maxx { maxx = x; }
                if y > maxy { maxy = y; }
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
            for _ in 0..total { let _ = it.next(); }
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
            for _ in 0..area_w { let _ = it.next(); }
        }

        // Rows in intersection
        for ry in 0..inter_h {
            // Skip left columns
            for _ in 0..left_skip { let _ = it.next(); }

            // Write visible span into FB
            let dst_row = (y0 as usize + ry) * fbw;
            let dst_off = dst_row + (x0 as usize);
            for cx in 0..take {
                if let Some(c) = it.next() {
                    self.fb[dst_off + cx] = c.into_storage();
                }
            }

            // Skip right columns
            for _ in 0..right_skip { let _ = it.next(); }
        }

        // Drain rows below to preserve iterator semantics
        let rows_below = area_h.saturating_sub(top_skip + inter_h);
        for _ in 0..rows_below {
            for _ in 0..area_w { let _ = it.next(); }
        }

        // One flush from FB (handles even-alignment + single RAMWR)
        let x1 = x0 + (take as u16) - 1;
        let y1 = y0 + (inter_h as u16) - 1;
        let _ = self.flush_fb_rect_even(x0, y0, x1, y1);

        Ok(())
    }

    fn clear(&mut self, color: embedded_graphics::pixelcolor::Rgb565) -> Result<(), Self::Error> {
        // Use fast fill rect
        let _ = self.fill_rect_solid(0, 0, self.w, self.h, color);
        Ok(())
    }

}

// Convenience builder that picks common defaults and returns the concrete type.
// Returning the concrete type lets display.rs use `impl Trait` to erase it later.
pub fn new_with_defaults<'fb, SPI, RST>(
    spi: SPI,
    rst: Option<RST>,
    delay: &mut impl embedded_hal::delay::DelayNs,
    fb: &'fb mut [u16],
) -> Result<Co5300Display<'fb, SPI, RST>, Co5300Error<SPI::Error, RST::Error>>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    let mut display = Co5300Display::new(spi, rst, delay, CO5300_WIDTH, CO5300_HEIGHT, fb)?;
    display.set_window_raw(0, 0, CO5300_WIDTH - 1, CO5300_HEIGHT - 1)?;
    Ok(display)
}

// This matches wiring: Spi<'a, Blocking> + CS pin + NoDelay
pub type SpiDev<'a> = ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>;

// Expose a ready-to-use display type (share lifetime with SPI and FB)
pub type DisplayType<'a> = Co5300Display<'a, SpiDev<'a>, Output<'a>>;
