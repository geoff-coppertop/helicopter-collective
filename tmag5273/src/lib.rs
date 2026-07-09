#![no_std]

pub mod types;

use embedded_hal_async::i2c::I2c;
use types::{DeviceId, DeviceVersion, MagData, ManufacturerId, TMag5273Error};

const DEFAULT_ADDRESS: u8 = 0x22;

/// TMAG5273 register addresses (TI datasheet, Section 7.6).
mod reg {
    pub const DEVICE_CONFIG_2: u8 = 0x01;
    pub const SENSOR_CONFIG_1: u8 = 0x02;
    pub const T_CONFIG: u8 = 0x07;
    pub const DEVICE_ID: u8 = 0x0D;
    pub const MANUFACTURER_ID_LSB: u8 = 0x0E;
    /// First result register — sequential read covers T/X/Y/Z (8 bytes).
    pub const T_MSB_RESULT: u8 = 0x10;
}

pub struct TMag5273<I2C> {
    i2c: I2C,
    address: u8,
    #[allow(dead_code)]
    version: DeviceVersion,
}

impl<I2C: I2c> TMag5273<I2C> {
    /// Create a driver instance using the default I2C address (0x22).
    pub fn new(i2c: I2C, version: DeviceVersion) -> Result<Self, TMag5273Error> {
        Ok(Self {
            i2c,
            address: DEFAULT_ADDRESS,
            version,
        })
    }

    /// Create a driver instance with a non-default I2C address.
    pub fn new_with_address(
        i2c: I2C,
        version: DeviceVersion,
        address: u8,
    ) -> Result<Self, TMag5273Error> {
        Ok(Self { i2c, address, version })
    }

    /// Consume the driver and return the underlying I2C bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Configure the sensor for continuous X/Y/Z + temperature measurement.
    ///
    /// Register writes (all other bits left at power-on defaults):
    /// - SENSOR_CONFIG_1 [7:4] MAG_CH_EN = 0b0111  → enable X, Y, Z
    /// - T_CONFIG        [0]   T_CH_EN   = 1        → enable temperature
    /// - DEVICE_CONFIG_2 [1:0] OPERATING_MODE = 0b10 → continuous measurement
    pub async fn init_default(mut self) -> Result<Self, TMag5273Error> {
        // Enable X, Y, Z channels (MAG_CH_EN bits [7:4] = 0b0111).
        self.write_register(reg::SENSOR_CONFIG_1, 0x70).await?;

        // Enable temperature channel (T_CH_EN bit [0] = 1).
        self.write_register(reg::T_CONFIG, 0x01).await?;

        // Switch to continuous measurement mode (OPERATING_MODE bits [1:0] = 0b10).
        // Do this last so the sensor starts measuring with the right config.
        self.write_register(reg::DEVICE_CONFIG_2, 0x02).await?;

        Ok(self)
    }

    /// Read a snapshot of all enabled channels (temperature, X, Y, Z).
    ///
    /// Performs a single 8-byte sequential read starting at T_MSB_RESULT.
    /// Values are raw 16-bit two's complement ADC counts.
    pub async fn get_all_data(&mut self) -> Result<MagData, TMag5273Error> {
        let mut buf = [0u8; 8];
        self.read_registers(reg::T_MSB_RESULT, &mut buf).await?;

        Ok(MagData {
            temperature: i16::from_be_bytes([buf[0], buf[1]]),
            x: i16::from_be_bytes([buf[2], buf[3]]),
            y: i16::from_be_bytes([buf[4], buf[5]]),
            z: i16::from_be_bytes([buf[6], buf[7]]),
        })
    }

    /// Read the DEVICE_ID register (0x0D).
    pub async fn get_device_id(&mut self) -> Result<DeviceId, TMag5273Error> {
        let val = self.read_register(reg::DEVICE_ID).await?;
        Ok(DeviceId(val))
    }

    /// Read the 16-bit manufacturer ID (registers 0x0E–0x0F, LSB first).
    ///
    /// Texas Instruments devices return 0x5449.
    pub async fn get_manufacturer_id(&mut self) -> Result<ManufacturerId, TMag5273Error> {
        let mut buf = [0u8; 2];
        self.read_registers(reg::MANUFACTURER_ID_LSB, &mut buf).await?;
        Ok(ManufacturerId(u16::from_le_bytes([buf[0], buf[1]])))
    }

    async fn read_register(&mut self, reg: u8) -> Result<u8, TMag5273Error> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.address, &[reg], &mut buf)
            .await
            .map_err(|_| TMag5273Error::I2c)?;
        Ok(buf[0])
    }

    async fn read_registers(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), TMag5273Error> {
        self.i2c
            .write_read(self.address, &[reg], buf)
            .await
            .map_err(|_| TMag5273Error::I2c)
    }

    async fn write_register(&mut self, reg: u8, val: u8) -> Result<(), TMag5273Error> {
        self.i2c
            .write(self.address, &[reg, val])
            .await
            .map_err(|_| TMag5273Error::I2c)
    }
}
