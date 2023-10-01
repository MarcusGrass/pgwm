use tiny_std::fs::OpenOptions;
use tiny_std::io::Read;
use tiny_std::UnixStr;

use crate::error::Error;

pub const BAT_FILE: &UnixStr = UnixStr::from_str_checked("/sys/class/power_supply/BAT0/capacity\0");

#[allow(unsafe_code)]
#[inline]
pub fn get_battery_percentage(buf: &mut [u8]) -> Result<u8, Error> {
    let mut file = OpenOptions::new().read(true).open(BAT_FILE)?;
    let bytes = file.read(buf)?;
    atoi::atoi(&buf[..bytes]).ok_or(Error::BatParseError)
}

#[inline]
pub fn parse_battery_percentage(buf: &[u8]) -> Result<u8, Error> {
    atoi::atoi(buf).ok_or(Error::BatParseError)
}
