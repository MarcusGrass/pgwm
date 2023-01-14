use tiny_std::fs::OpenOptions;
use tiny_std::io::Read;

use crate::error::Error;

pub const BAT_FILE: &str = "/sys/class/power_supply/BAT0/capacity\0";

#[derive(Debug, Default)]
pub struct BatChecker {
    read_buf: [u8; 8],
}

impl BatChecker {
    #[inline]
    pub fn get_battery_percentage(&mut self) -> Result<u8, Error> {
        get_battery_percentage(&mut self.read_buf)
    }
}

#[allow(unsafe_code)]
#[inline]
pub fn get_battery_percentage(buf: &mut [u8]) -> Result<u8, Error> {
    let mut file = OpenOptions::new().read(true).open(BAT_FILE)?;
    let bytes = file.read(buf)?;
    atoi::atoi(&buf[..bytes]).ok_or(Error::BatParseError)
}
