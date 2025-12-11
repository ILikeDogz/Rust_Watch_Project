//! Minimal QMI8658 IMU bring-up and a simple smash detector
//! The register values are conservative defaults for the Waveshare ESP32-S3
//! Touch AMOLED 1.43" board (QMI8658 on the touch I2C bus)

use embedded_hal::i2c;

pub const DEFAULT_I2C_ADDR: u8 = 0x6B; // AD0 pulled high on the Waveshare board

const REG_WHO_AM_I: u8 = 0x00;
const REG_CTRL1: u8 = 0x02; // accel config
const REG_CTRL2: u8 = 0x03; // gyro config
const REG_CTRL7: u8 = 0x08; // power / enable
const REG_CTRL8: u8 = 0x09; // reset/power settings
// const REG_STATUS_INT: u8 = 0x2D;
// const REG_STATUS0: u8 = 0x2E;
const REG_ACC_START: u8 = 0x35; // AX_L .. GZ_H
const INT_ENABLE_BITS: u8 = 0x18; // INT1_ENABLE (0x08) | INT2_ENABLE (0x10) per qmi8658c.h
const CTRL8_DATAVALID_INT1: u8 = 0x40; // route data-ready to INT1

// Expected chip ID for QMI8658. Some revisions report 0x05 or 0x0F; keep it loose.
const WHO_AM_I_FALLBACK: u8 = 0x05;
const WHO_AM_I_ALT: u8 = 0x0F;

#[derive(Clone, Copy, Debug)]
pub struct ImuSample {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
}

impl ImuSample {
    #[inline]
    pub fn accel_mag_sq(&self) -> i64 {
        self.accel
            .iter()
            .map(|v| {
                let v = *v as i64;
                v * v
            })
            .sum()
    }

    #[inline]
    pub fn gyro_mag_sq(&self) -> i64 {
        self.gyro
            .iter()
            .map(|v| {
                let v = *v as i64;
                v * v
            })
            .sum()
    }
}

// IMU error type
#[derive(Debug)]
pub enum ImuError<E> {
    Bus(E),
    BadWhoAmI(u8),
}

// Allow automatic conversion from I2C errors
impl<E> From<E> for ImuError<E> {
    fn from(e: E) -> Self {
        ImuError::Bus(e)
    }
}

// QMI8658 IMU driver
pub struct Qmi8658<I2C> {
    i2c: I2C,
    address: u8,
}

// Implement driver methods
impl<I2C> Qmi8658<I2C>
where
    I2C: i2c::ErrorType + i2c::I2c,
{
    // Create a new instance and initialize the IMU
    pub fn new(i2c: I2C, address: u8) -> Result<Self, ImuError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.init()?;
        Ok(this)
    }

    // Read WHO_AM_I register
    pub fn who_am_i(&mut self) -> Result<u8, ImuError<I2C::Error>> {
        self.read_reg(REG_WHO_AM_I)
    }

    // Initialize the IMU with default settings
    fn init(&mut self) -> Result<(), ImuError<I2C::Error>> {
        let who = self.who_am_i()?;
        if who != WHO_AM_I_FALLBACK && who != WHO_AM_I_ALT {
            // allow init to continue so users can still probe, but surface an error
            return Err(ImuError::BadWhoAmI(who));
        }

        // Soft reset and clear low-power.
        // Ignore errors here to avoid blocking subsequent config steps.
        let _ = self.write_reg(REG_CTRL8, 0x10);

        // Accelerometer: +/-8g, ~1 kHz ODR (0x60 per datasheet examples), enable INT1/INT2
        let _ = self.write_reg(REG_CTRL1, 0x60 | INT_ENABLE_BITS);
        // Gyro: +/-512 dps, ~1 kHz ODR (0x64 per datasheet examples)
        let _ = self.write_reg(REG_CTRL2, 0x64);

        // Enable accel + gyro, set to Active
        self.write_reg(REG_CTRL7, 0x03)?;

        // Route data-ready to INT1 (GPIO8) so we get an interrupt per sample.
        let _ = self.write_reg(REG_CTRL8, CTRL8_DATAVALID_INT1);

        Ok(())
    }

    // Read an 8-bit register
    pub fn read_reg8(&mut self, reg: u8) -> Result<u8, ImuError<I2C::Error>> {
        self.read_reg(reg)
    }

    // Write an 8-bit register
    fn write_reg(&mut self, reg: u8, val: u8) -> Result<(), ImuError<I2C::Error>> {
        self.i2c
            .write(self.address, &[reg, val])
            .map_err(ImuError::Bus)
    }

    // Read an 8-bit register
    fn read_reg(&mut self, reg: u8) -> Result<u8, ImuError<I2C::Error>> {
        let mut out = [0u8];
        self.i2c
            .write_read(self.address, &[reg], &mut out)
            .map_err(ImuError::Bus)?;
        Ok(out[0])
    }

    // Read a sample (accel + gyro)
    pub fn read_sample(&mut self) -> Result<ImuSample, ImuError<I2C::Error>> {
        let mut buf = [0u8; 12];
        self.i2c
            .write_read(self.address, &[REG_ACC_START], &mut buf)
            .map_err(ImuError::Bus)?;

        let accel = [
            i16::from_le_bytes([buf[0], buf[1]]),
            i16::from_le_bytes([buf[2], buf[3]]),
            i16::from_le_bytes([buf[4], buf[5]]),
        ];
        let gyro = [
            i16::from_le_bytes([buf[6], buf[7]]),
            i16::from_le_bytes([buf[8], buf[9]]),
            i16::from_le_bytes([buf[10], buf[11]]),
        ];

        Ok(ImuSample { accel, gyro })
    }

    // Consume the driver and return the underlying I2C bus
    pub fn into_inner(self) -> I2C {
        self.i2c
    }
}

// Simple smash detector using acceleration magnitude and rise detection
pub struct SmashDetector {
    threshold_sq: i64,
    rise_threshold_sq: i64,
    freefall_sq: i64,
    gyro_limit_sq: i64,
    // Require one axis to dominate others (to reject swings that are multi-axis noisy)
    axis_ratio_num: i32,
    axis_ratio_den: i32,
    cooldown_ms: u32,
    last_mag_sq: i64,
    last_freefall: bool,
    last_trigger_ms: u64,
    gravity_dir: [i32; 3],
    gravity_samples: u16,
    baseline_mag_sq: i64,
    gravity_mag_sq: i64,
    baseline_dot: i64,
    last_dot: i64,
}

// Implement smash detector methods
impl SmashDetector {
    pub fn new(
        threshold_raw: i32,
        rise_raw: i32,
        gyro_limit_raw: i32,
        freefall_raw: i32,
        cooldown_ms: u32,
    ) -> Self {
        Self {
            threshold_sq: (threshold_raw as i64) * (threshold_raw as i64),
            rise_threshold_sq: (rise_raw as i64) * (rise_raw as i64),
            freefall_sq: (freefall_raw as i64) * (freefall_raw as i64),
            gyro_limit_sq: (gyro_limit_raw as i64) * (gyro_limit_raw as i64),
            axis_ratio_num: 0,
            axis_ratio_den: 1,
            cooldown_ms,
            last_mag_sq: 0,
            last_freefall: false,
            last_trigger_ms: 0,
            gravity_dir: [0; 3],
            gravity_samples: 0,
            baseline_mag_sq: 0,
            gravity_mag_sq: 0,
            baseline_dot: 0,
            last_dot: 0,
        }
    }

    // Default rough smash detector profile
    pub fn default_rough() -> Self {
        // Raw units tuned for observed ~1000 counts per 1g on the Waveshare board (8g range).
        // Re-tighten slightly: ~1.8g threshold, ~0.7g rise, gyro gate ~60k, cooldown 160 ms.
        let mut s = Self::new(1_800, 700, 60_000, 200, 160);
        // Require a dominant axis (at least ~2:1 over others) once enabled.
        s.axis_ratio_num = 2;
        s.axis_ratio_den = 1;
        s
    }

    // Update with a new sample, return true if a smash event is detected
    pub fn update(&mut self, now_ms: u64, sample: &ImuSample) -> bool {
        let mag_sq = sample.accel_mag_sq();
        let gyro_sq = sample.gyro_mag_sq();
        let in_cooldown = now_ms.saturating_sub(self.last_trigger_ms) < self.cooldown_ms as u64;

        // Freefall guard: if the previous sample was near zero-g, treat the spike as a drop.
        let freefall_guard = self.last_freefall;
        self.last_freefall = mag_sq < self.freefall_sq;

        let rising_fast = mag_sq.saturating_sub(self.last_mag_sq) >= self.rise_threshold_sq;
        self.last_mag_sq = mag_sq;

        // Learn gravity direction quickly when movement is small.
        if self.gravity_samples < u16::MAX {
            if mag_sq > 600_000 && mag_sq < 4_000_000 {
                let k = (self.gravity_samples as i64).saturating_add(1);
                for i in 0..3 {
                    self.gravity_dir[i] = (((self.gravity_dir[i] as i64)
                        * self.gravity_samples as i64
                        + sample.accel[i] as i64)
                        / k) as i32;
                }
                if self.gravity_samples < 64 {
                    self.gravity_samples += 1;
                }
                if self.gravity_samples >= 8 && self.gravity_mag_sq == 0 {
                    self.gravity_mag_sq = self
                        .gravity_dir
                        .iter()
                        .map(|v| {
                            let vv = *v as i64;
                            vv * vv
                        })
                        .sum();
                    self.baseline_dot = self.gravity_mag_sq;
                    self.last_dot = self.baseline_dot;
                }
            }
        }

        // Axis bias check: projection should move further along gravity than the baseline (smash down).
        let mut axis_ok = true;
        if self.gravity_mag_sq > 0 {
            let dot: i64 = (sample.accel[0] as i64 * self.gravity_dir[0] as i64)
                + (sample.accel[1] as i64 * self.gravity_dir[1] as i64)
                + (sample.accel[2] as i64 * self.gravity_dir[2] as i64);
            let delta = dot.saturating_sub(self.baseline_dot); // positive if more along gravity
            let rise_min = self.gravity_mag_sq / 2; // need ~0.5g^2 additional projection
            let dot_rise_min = self.rise_threshold_sq / 2;
            axis_ok = (dot * self.baseline_dot) > 0 // same general direction as gravity
                && delta >= rise_min
                && (dot - self.last_dot) >= dot_rise_min;
            self.last_dot = dot;
        }

        // Baseline magnitude (|a|^2) EMA for shake rejection: only update when gyro is quiet.
        if gyro_sq < 10_000 && mag_sq > 500_000 && mag_sq < 2_500_000 {
            if self.baseline_mag_sq == 0 {
                self.baseline_mag_sq = mag_sq;
            } else {
                // EMA with alpha ~1/16
                self.baseline_mag_sq = ((self.baseline_mag_sq * 15) + mag_sq) / 16;
            }
        }

        // Dominant axis check: max axis at least ratio over others.
        let mut ratio_ok = true;
        if self.axis_ratio_num > 0 {
            let mut axes = [
                sample.accel[0].abs() as i32,
                sample.accel[1].abs() as i32,
                sample.accel[2].abs() as i32,
            ];
            axes.sort_unstable();
            let max = axes[2] as i64;
            let mid = axes[1] as i64;
            let lo = axes[0] as i64;
            let num = self.axis_ratio_num as i64;
            let den = self.axis_ratio_den as i64;
            ratio_ok = max * den >= mid * num && max * den >= lo * num;
        }

        // Gyro check: allow high gyro if accel is very high, otherwise enforce limit.
        let gyro_ok = if mag_sq > self.threshold_sq.saturating_mul(4) {
            true
        } else {
            gyro_sq < self.gyro_limit_sq
        };

        // Require a sharp jump over baseline to reject slow wiggles.
        let mut jump_ok = true;
        if self.baseline_mag_sq > 0 {
            // need mag_sq at least 4x baseline to count as smash
            jump_ok = mag_sq.saturating_mul(1) > self.baseline_mag_sq.saturating_mul(4);
        }

        let hit = !in_cooldown
            && !freefall_guard
            && mag_sq >= self.threshold_sq
            && rising_fast
            && gyro_ok
            && axis_ok
            && ratio_ok
            && jump_ok;

        if hit {
            self.last_trigger_ms = now_ms;
        }

        hit
    }

    // Compute the dot product of the sample acceleration with the learned gravity direction
    pub fn gravity_dot(&self, sample: &ImuSample) -> i64 {
        (sample.accel[0] as i64 * self.gravity_dir[0] as i64)
            + (sample.accel[1] as i64 * self.gravity_dir[1] as i64)
            + (sample.accel[2] as i64 * self.gravity_dir[2] as i64)
    }
}
