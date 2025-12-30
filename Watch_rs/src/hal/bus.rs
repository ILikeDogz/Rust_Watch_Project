// Shared I2C bus type definitions

use esp_hal::i2c::master::I2c;

type SharedI2c = I2c<'static, esp_hal::Blocking>;
pub type SharedI2cRef = &'static core::cell::RefCell<SharedI2c>;
pub type SharedI2cDev = embedded_hal_bus::i2c::RefCellDevice<'static, SharedI2c>;
