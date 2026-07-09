//! Converts the TMAG5273's raw 16-bit ADC counts into engineering units
//! (mT, °C). Pure arithmetic with no embedded-only dependencies, so it's
//! covered by `cargo test-host` rather than only exercisable on hardware.

/// Magnetic flux density conversion factor, in mT per raw LSB.
///
/// The driver's `init_default` never touches SENSOR_CONFIG_2, so the sensor
/// is left at its power-on default ±40 mT range: the 16-bit signed reading
/// spans the full 80 mT width. If you reconfigure the sensor for the
/// ±80 mT range, this must be doubled (see the TI TMAG5273 datasheet,
/// Table 7-11).
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

/// TMAG5273 X/Y/Z magnetic flux density (mT) and temperature (°C), converted
/// from the driver's raw 16-bit ADC counts.
pub struct EngineeringUnits {
    pub x_mt: f32,
    pub y_mt: f32,
    pub z_mt: f32,
    pub temperature_c: f32,
}

impl EngineeringUnits {
    /// Converts raw sensor readings (as returned by
    /// `tmag5273::TMag5273::get_all_data`) into engineering units.
    pub fn from_raw(x: i16, y: i16, z: i16, temperature: i16) -> Self {
        Self {
            x_mt: x as f32 * MT_PER_LSB,
            y_mt: y as f32 * MT_PER_LSB,
            z_mt: z as f32 * MT_PER_LSB,
            temperature_c: (temperature as f32 - T_ADC_T0) / T_SENSITIVITY + T_REF_C,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_converts_zero_to_mid_scale_and_reference_temperature() {
        // raw=0 sits at mid-scale (0 mT); raw=T_ADC_T0 is exactly the 25°C
        // reference point.
        let u = EngineeringUnits::from_raw(0, 0, 0, 17303);
        assert!((u.x_mt - 0.0).abs() < 1e-6);
        assert!((u.y_mt - 0.0).abs() < 1e-6);
        assert!((u.z_mt - 0.0).abs() < 1e-6);
        assert!((u.temperature_c - 25.0).abs() < 1e-4);
    }

    #[test]
    fn from_raw_converts_full_scale_codes_to_plus_minus_40_mt() {
        let u = EngineeringUnits::from_raw(32767, -32768, 0, 17303);
        assert!((u.x_mt - 40.0).abs() < 0.01);
        assert!((u.y_mt - (-40.0)).abs() < 0.01);
    }

    #[test]
    fn from_raw_does_not_regress_to_the_314_degree_bug() {
        // Regression test: an earlier version computed temperature as
        // `raw / T_SENSITIVITY + 25.0`, omitting the T_ADC_T0 subtraction,
        // and reported ~314°C in a normal room. A raw code near T_ADC_T0
        // must land near room temperature, not hundreds of degrees off.
        let u = EngineeringUnits::from_raw(0, 0, 0, 17123);
        assert!(u.temperature_c > 15.0 && u.temperature_c < 35.0);
    }
}
