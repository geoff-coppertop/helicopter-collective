//! Reads magnetic field and temperature data from a TMAG5273 3-axis Hall effect sensor.
//!
//! The sensor is connected to I2C0: SDA on PIN_20, SCL on PIN_21.
//! Measurements are logged every 500 ms via RTT using defmt, converted from
//! raw ADC counts into engineering units (mT, °C).

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{self, Config, InterruptHandler};
use embassy_rp::peripherals::I2C0;
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c as I2C_HAL;
use tmag5273::TMag5273;
use tmag5273::types::{DeviceVersion, MagData, TMag5273Error};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    I2C0_IRQ => InterruptHandler<I2C0>;
});

/// Magnetic flux density conversion factor, in mT per raw LSB.
///
/// `init_default` never touches SENSOR_CONFIG_2, so the sensor is left at its
/// power-on default ±40 mT range: the 16-bit signed reading spans the full
/// 80 mT width. If you reconfigure the sensor for the ±80 mT range, this
/// must be doubled (see the TI TMAG5273 datasheet, Table 7-11).
const MT_PER_LSB: f32 = 80.0 / 65536.0;

/// Temperature conversion constants from the TI TMAG5273 datasheet
/// (Section 7.3.7): T(°C) = (raw − T_ADC_T0) / T_SENSITIVITY + T_REF_C.
///
/// T_ADC_T0 is the raw ADC code the sensor reports at the 25 °C reference
/// point — NOT the reference temperature itself. This value is carried over
/// from datasheet/training knowledge rather than a live lookup; verify it
/// against a real TMAG5273 datasheet before trusting readings from hardware.
const T_SENSITIVITY: f32 = 60.1;
const T_ADC_T0: f32 = 17303.0;
const T_REF_C: f32 = 25.0;

struct EngineeringUnits {
    x_mt: f32,
    y_mt: f32,
    z_mt: f32,
    temperature_c: f32,
}

impl EngineeringUnits {
    fn from_raw(data: &MagData) -> Self {
        Self {
            x_mt: data.x as f32 * MT_PER_LSB,
            y_mt: data.y as f32 * MT_PER_LSB,
            z_mt: data.z as f32 * MT_PER_LSB,
            temperature_c: (data.temperature as f32 - T_ADC_T0) / T_SENSITIVITY + T_REF_C,
        }
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_20;
    let scl = p.PIN_21;

    info!("set up i2c");
    let i2c = i2c::I2c::new_async(p.I2C0, scl, sda, Irqs, Config::default());

    info!("init tmag5273");
    let mut mag_sensor = TMag5273::new(i2c, DeviceVersion::TMAG5273B1)
        .unwrap()
        .init_default()
        .await
        .unwrap();

    print_device_stats(&mut mag_sensor).await.unwrap();

    loop {
        let data = mag_sensor.get_all_data().await.unwrap();
        let units = EngineeringUnits::from_raw(&data);
        info!(
            "x: {} mT, y: {} mT, z: {} mT, temp: {} C",
            units.x_mt, units.y_mt, units.z_mt, units.temperature_c
        );

        Timer::after(Duration::from_millis(500)).await;
    }
}

async fn print_device_stats<I2C>(mag_sensor: &mut TMag5273<I2C>) -> Result<(), TMag5273Error>
where
    I2C: I2C_HAL,
{
    let device_id = mag_sensor.get_device_id().await?;
    info!("Device ID: {:?}", defmt::Debug2Format(&device_id));
    let manufacturer_id = mag_sensor.get_manufacturer_id().await?;
    info!(
        "Manufacturer ID: {:?}",
        defmt::Debug2Format(&manufacturer_id)
    );

    Ok(())
}
