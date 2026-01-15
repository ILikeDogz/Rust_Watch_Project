// Module for initializing IMU over a shared I2C bus

use crate::drivers::qmi8658_imu::{Qmi8658, DEFAULT_I2C_ADDR};
use crate::hal::bus::{SharedI2cDev, SharedI2cRef};
use embedded_hal::i2c::I2c as _;

pub fn init_imu(bus: SharedI2cRef) -> Option<Qmi8658<SharedI2cDev>> {
    let mut bus_device = embedded_hal_bus::i2c::RefCellDevice::new(bus);

    // Small helper to probe addresses
    let mut probe = |addr: u8| -> Option<u8> {
        let mut who = [0u8];
        match bus_device.write_read(addr, &[0x00], &mut who) {
            Ok(()) => Some(who[0]),
            Err(_e) => None,
        }
    };

    // First attempt
    let mut found = None;
    for &addr in &[DEFAULT_I2C_ADDR, 0x6A] {
        if let Some(who) = probe(addr) {
            found = Some((addr, who));
            break;
        }
    }

    // If not found, wait and re-probe (handles power-up race)
    if found.is_none() {
        for _ in 0..50 {
            core::hint::spin_loop();
        }
        for &addr in &[DEFAULT_I2C_ADDR, 0x6A] {
            if let Some(who) = probe(addr) {
                found = Some((addr, who));
                break;
            }
        }
    }

    if let Some((addr, _who)) = found {
        match Qmi8658::new(bus_device, addr) {
            Ok(dev) => Some(dev),
            Err(_e) => None,
        }
    } else {
        None
    }
}
