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
use helicopter_collective::filter::{Ema, round_to};
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

/// Per-axis EMA smoothing factors, independently tunable since each is a
/// physically distinct signal with its own responsiveness needs.
///
/// X/Y/Z are the live "3D mouse" input signal, so responsiveness matters.
/// Two hardware traces so far:
///   - alpha=0.2: a magnet swipe took ~40 samples (~20 s, time constant
///     ≈ 2.2 s) to settle — mostly filter lag, not physical motion.
///   - alpha=0.4: settling after the last disturbance dropped to ~10
///     samples (~5 s, time constant ≈ 1.0 s) — better, but still sluggish
///     for live tracking. Baseline noise stayed under ~0.15 mT with room
///     to spare.
/// 0.6 (time constant ≈ 0.55 s) trades further noise margin for lag;
/// re-capture a trace after this change to confirm it's still acceptable
/// — a fixed-alpha EMA can't buy responsiveness without giving up some
/// jitter rejection, since one alpha controls both.
///
/// Temperature has no low-latency requirement — nothing reads it for
/// control — and real temperature changes (e.g. touching the sensor) are
/// already slow relative to the sample rate, so it can trade responsiveness
/// for cleaner readings independently of the magnetic axes' tuning.
const X_FILTER_ALPHA: f32 = 0.6;
const Y_FILTER_ALPHA: f32 = 0.6;
const Z_FILTER_ALPHA: f32 = 0.6;
const TEMP_FILTER_ALPHA: f32 = 0.1;

/// Applies an independent [`Ema`] instance to each field of
/// [`EngineeringUnits`]: same filter, reused per axis, each with its own
/// tunable alpha.
struct EngineeringUnitsFilter {
    x: Ema,
    y: Ema,
    z: Ema,
    temperature: Ema,
}

impl EngineeringUnitsFilter {
    const fn new() -> Self {
        Self {
            x: Ema::new(X_FILTER_ALPHA),
            y: Ema::new(Y_FILTER_ALPHA),
            z: Ema::new(Z_FILTER_ALPHA),
            temperature: Ema::new(TEMP_FILTER_ALPHA),
        }
    }

    fn update(&mut self, sample: &EngineeringUnits) -> EngineeringUnits {
        EngineeringUnits {
            x_mt: self.x.update(sample.x_mt),
            y_mt: self.y.update(sample.y_mt),
            z_mt: self.z.update(sample.z_mt),
            temperature_c: self.temperature.update(sample.temperature_c),
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

    let mut filter = EngineeringUnitsFilter::new();

    loop {
        let data = mag_sensor.get_all_data().await.unwrap();
        let units = filter.update(&EngineeringUnits::from_raw(&data));
        info!(
            "x: {} mT, y: {} mT, z: {} mT, temp: {} C",
            round_to(units.x_mt, 1000.0),
            round_to(units.y_mt, 1000.0),
            round_to(units.z_mt, 1000.0),
            round_to(units.temperature_c, 10.0)
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
