use crate::error::Error;

const BAT_FILE: &str = "/sys/class/power_supply/BAT0/capacity";

pub fn get_battery_percentage() -> Result<u8, Error> {
    let content = std::fs::read(BAT_FILE)?;
    atoi::atoi(&content).ok_or(Error::BatParseError)
}
