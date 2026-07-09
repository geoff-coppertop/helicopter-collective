//! Reads magnetic field and temperature data from a TMAG5273 3-axis Hall effect sensor.
//!
//! The sensor is connected to I2C0: SDA on PIN_20, SCL on PIN_21.
//! Measurements are logged every 500 ms via RTT using defmt, converted from
//! raw ADC counts into engineering units (mT, °C). Each line logs both the
//! raw (pre-filter) and filtered values, one line of column names printed
//! once up front, so a captured trace carries enough to tune the filter
//! without having to reverse-engineer raw noise from filtered output.

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
use helicopter_collective::filter::{Ema, OneEuroFilter, round_to};
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

/// Must match the actual sample interval in the loop below (500 ms).
const SAMPLE_PERIOD_S: f32 = 0.5;

/// One Euro Filter parameters for the magnetic axes — see [`OneEuroFilter`]
/// and Casiez, Roussel & Vogel, "1€ Filter: A Simple Speed-based Low-pass
/// Filter for Noisy Input in Interactive Systems" (CHI 2012). Two rounds of
/// fixed-alpha Ema tuning (0.2 → 0.4 → 0.6) kept running into the same
/// limit: one alpha trades resting jitter against motion lag, and there's
/// no setting that's both quiet at rest and responsive during a swipe. This
/// filter adapts its cutoff to the signal's estimated speed instead of
/// fixing one tradeoff for all speeds.
///
/// Starting points, not yet hardware-validated — see the raw/filtered log
/// output below for the data to validate them against:
/// - MAG_MINCUTOFF_HZ ≈ 0.15 Hz: resting smoothing roughly as heavy as our
///   gentlest fixed-alpha trial (alpha=0.2 ⇒ ≈0.08 Hz cutoff at this sample
///   rate), with a bit less margin since fast motion no longer has to fight
///   through the same cutoff to get through.
/// - MAG_BETA ≈ 0.1: derived from swipe-event derivatives of roughly
///   5–15 mT/s seen in *filtered* traces so far — true raw-signal
///   derivatives are almost certainly larger, hence logging raw alongside
///   filtered now. Pushes the cutoff toward ~1–2 Hz during a real swipe.
/// - MAG_DCUTOFF_HZ = 1.0: the paper's standard default for smoothing the
///   speed estimate itself; rarely needs tuning.
const MAG_MINCUTOFF_HZ: f32 = 0.15;
const MAG_BETA: f32 = 0.1;
const MAG_DCUTOFF_HZ: f32 = 1.0;

/// Temperature keeps a fixed-alpha [`Ema`]: nothing reads temperature for
/// low-latency control, so there's no reason to let it speed up during fast
/// changes the way the magnetic axes need to.
const TEMP_FILTER_ALPHA: f32 = 0.1;

/// Filters each field of [`EngineeringUnits`] independently: [`OneEuroFilter`]
/// for the magnetic axes (need to track fast motion without resting jitter),
/// a fixed-alpha [`Ema`] for temperature (no responsiveness requirement).
struct EngineeringUnitsFilter {
    x: OneEuroFilter,
    y: OneEuroFilter,
    z: OneEuroFilter,
    temperature: Ema,
}

impl EngineeringUnitsFilter {
    fn new() -> Self {
        let mag_filter =
            || OneEuroFilter::new(MAG_MINCUTOFF_HZ, MAG_BETA, MAG_DCUTOFF_HZ, SAMPLE_PERIOD_S);
        Self {
            x: mag_filter(),
            y: mag_filter(),
            z: mag_filter(),
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

    info!("raw_x raw_y raw_z raw_t | filt_x filt_y filt_z filt_t (mT mT mT C)");

    loop {
        let data = mag_sensor.get_all_data().await.unwrap();
        let raw = EngineeringUnits::from_raw(&data);
        let filtered = filter.update(&raw);
        info!(
            "{} {} {} {} | {} {} {} {}",
            round_to(raw.x_mt, 1000.0),
            round_to(raw.y_mt, 1000.0),
            round_to(raw.z_mt, 1000.0),
            round_to(raw.temperature_c, 10.0),
            round_to(filtered.x_mt, 1000.0),
            round_to(filtered.y_mt, 1000.0),
            round_to(filtered.z_mt, 1000.0),
            round_to(filtered.temperature_c, 10.0),
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
