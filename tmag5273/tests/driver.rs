use embedded_hal_mock::eh1::i2c::{Mock, Transaction};
use tmag5273::types::DeviceVersion;
use tmag5273::TMag5273;

const ADDR: u8 = 0x22;

// Register addresses mirrored here so a typo in the driver shows up as a test failure.
const REG_DEVICE_CONFIG_2: u8 = 0x01;
const REG_SENSOR_CONFIG_1: u8 = 0x02;
const REG_T_CONFIG: u8 = 0x07;
const REG_DEVICE_ID: u8 = 0x0D;
const REG_MANUFACTURER_ID_LSB: u8 = 0x0E;
const REG_T_MSB_RESULT: u8 = 0x10;

/// init_default must write three registers in this exact order:
///   1. SENSOR_CONFIG_1 = 0x70 (enable X/Y/Z channels)
///   2. T_CONFIG        = 0x01 (enable temperature channel)
///   3. DEVICE_CONFIG_2 = 0x02 (continuous measurement mode — last, so config is locked in first)
#[tokio::test]
async fn init_default_writes_registers_in_order() {
    let expectations = [
        Transaction::write(ADDR, vec![REG_SENSOR_CONFIG_1, 0x70]),
        Transaction::write(ADDR, vec![REG_T_CONFIG, 0x01]),
        Transaction::write(ADDR, vec![REG_DEVICE_CONFIG_2, 0x02]),
    ];
    let sensor = TMag5273::new(Mock::new(&expectations), DeviceVersion::TMAG5273B1)
        .unwrap()
        .init_default()
        .await
        .unwrap();
    sensor.release().done();
}

/// get_all_data must assemble the 8-byte burst (T/X/Y/Z, big-endian) into
/// the correct signed 16-bit fields.
#[tokio::test]
async fn get_all_data_assembles_be_i16_fields() {
    let raw = vec![
        0x01, 0x00, // temperature = 0x0100 =  256
        0xFF, 0xF0, // x          = 0xFFF0 = -16
        0x00, 0x10, // y          = 0x0010 =  16
        0x7F, 0xFF, // z          = 0x7FFF =  32767
    ];
    let expectations = [Transaction::write_read(ADDR, vec![REG_T_MSB_RESULT], raw)];
    let mut sensor = TMag5273::new(Mock::new(&expectations), DeviceVersion::TMAG5273B1).unwrap();
    let data = sensor.get_all_data().await.unwrap();
    assert_eq!(data.temperature, 256);
    assert_eq!(data.x, -16);
    assert_eq!(data.y, 16);
    assert_eq!(data.z, 32767);
    sensor.release().done();
}

/// get_device_id must target register 0x0D and return the byte verbatim.
#[tokio::test]
async fn get_device_id_reads_register_0x0d() {
    let expectations = [Transaction::write_read(ADDR, vec![REG_DEVICE_ID], vec![0x01])];
    let mut sensor = TMag5273::new(Mock::new(&expectations), DeviceVersion::TMAG5273B1).unwrap();
    let id = sensor.get_device_id().await.unwrap();
    assert_eq!(id.0, 0x01);
    sensor.release().done();
}

/// get_manufacturer_id must read two bytes from 0x0E and assemble them
/// little-endian. TI chips return 0x49 ('I') then 0x54 ('T') → 0x5449.
#[tokio::test]
async fn get_manufacturer_id_assembles_le_u16() {
    let expectations = [Transaction::write_read(
        ADDR,
        vec![REG_MANUFACTURER_ID_LSB],
        vec![0x49, 0x54],
    )];
    let mut sensor = TMag5273::new(Mock::new(&expectations), DeviceVersion::TMAG5273B1).unwrap();
    let id = sensor.get_manufacturer_id().await.unwrap();
    assert_eq!(id.0, 0x5449);
    sensor.release().done();
}
