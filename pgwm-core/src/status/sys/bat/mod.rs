use crate::error::Error;
use std::io::Read;

const BAT_FILE: &str = "/sys/class/power_supply/BAT0/capacity";

pub fn get_battery_percentage() -> Result<u8, Error> {
    let mut buf = [0u8; 8];
    let mut f = std::fs::File::open(BAT_FILE)?;
    f.read_exact(&mut buf)?;
    atoi::atoi(&buf).ok_or(Error::BatParseError)
}
