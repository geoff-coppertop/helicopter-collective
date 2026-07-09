#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceVersion {
    TMAG5273A1,
    TMAG5273B1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TMag5273Error {
    I2c,
    InvalidData,
}

/// Raw magnetic field and temperature readings from the sensor.
///
/// X/Y/Z are 16-bit two's complement ADC counts. Apply the appropriate
/// sensitivity factor (mT/LSB) for your range setting to convert to mT.
///
/// Temperature: T(°C) ≈ (raw − T_ADC_T0) / 60.1 + 25, where T_ADC_T0 is the
/// raw ADC code at the 25 °C reference point (not zero — see the TI
/// TMAG5273 datasheet, Section 7.3.7, for the exact offset).
#[derive(Debug, Clone, Copy)]
pub struct MagData {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub temperature: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceId(pub u8);

/// Manufacturer ID — Texas Instruments devices return 0x5449 ('TI').
#[derive(Debug, Clone, Copy)]
pub struct ManufacturerId(pub u16);
