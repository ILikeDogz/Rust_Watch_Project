// Functions for initializing and sharing an I2C bus for peripherals like IMU and RTC

extern crate alloc;

use alloc::boxed::Box;
use esp_hal::{
    i2c::master::{Config as I2cConfig, I2c},
    time::Rate,
};

use crate::hal::bus::SharedI2cRef;

pub fn init_shared_i2c_bus(
    i2c0: esp_hal::peripherals::I2C0<'static>,
    imu_i2c: crate::wiring::ImuI2cPins<'static>,
) -> Option<SharedI2cRef> {
    let cfg = I2cConfig::default().with_frequency(Rate::from_khz(400));
    match I2c::new(i2c0, cfg) {
        Ok(i2c) => {
            let i2c = i2c.with_sda(imu_i2c.sda).with_scl(imu_i2c.scl);
            let bus = core::cell::RefCell::new(i2c);
            Some(Box::leak(Box::new(bus)))
        }
        Err(_e) => None,
    }
}
