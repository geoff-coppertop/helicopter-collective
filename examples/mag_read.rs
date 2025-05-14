//! This example shows how to communicate asynchronous using i2c with external chips.
//!
//! Example written for the [`MCP23017 16-Bit I2C I/O Expander with Serial Interface`] chip.
//! (https://www.microchip.com/en-us/product/mcp23017)

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{self, Config, InterruptHandler};
use embassy_rp::peripherals::I2C0;
use embassy_time::{Duration, Timer};
use embedded_hal::i2c::I2c as I2C_HAL;
use tmag5273::TMag5273;
use tmag5273::types::{DeviceVersion, TMag5273Error};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    I2C0_IRQ => InterruptHandler<I2C0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_20;
    let scl = p.PIN_21;

    info!("set up i2c ");
    let i2c = i2c::I2c::new_async(p.I2C0, scl, sda, Irqs, Config::default());

    info!("init tmag5273");
    let mut mag_sensor = TMag5273::new(i2c, DeviceVersion::TMAG5273B1)
        .unwrap()
        .init_default()
        .unwrap();

    print_device_stats(&mut mag_sensor).unwrap();

    // Loop round and get some data
    loop {
        let data = mag_sensor.get_all_data().unwrap();
        info!("data: {:?}", defmt::Debug2Format(&data));

        Timer::after(Duration::from_millis(500)).await;
    }
}

// Print out some device starts by reading the Device ID and Manufacturer ID, panic if it cant be done
fn print_device_stats<I2C>(mag_sensor: &mut TMag5273<I2C>) -> Result<(), TMag5273Error>
where
    I2C: I2C_HAL,
{
    let device_id = mag_sensor.get_device_id()?;
    info!("Device ID: {:?}", defmt::Debug2Format(&device_id));
    let manufacturer_id = mag_sensor.get_manufacturer_id()?;
    info!(
        "Manufacturer ID: {:?}",
        defmt::Debug2Format(&manufacturer_id)
    );

    Ok(())
}
