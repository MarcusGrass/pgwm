use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "config-file")]
use serde::de::Error as DeError;
use time::{Month, OffsetDateTime, UtcOffset, Weekday};
use tiny_std::time::SystemTime;

use crate::error::Error;

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
pub struct ClockFormatter {
    #[cfg_attr(feature = "config-file", serde(deserialize_with = "from_pattern"))]
    format: Format,
    #[cfg_attr(feature = "config-file", serde(deserialize_with = "from_hms_tuple"))]
    utc_offset: UtcOffset,
}

impl ClockFormatter {
    pub fn format_now(&self) -> crate::error::Result<String> {
        let nanos = SystemTime::now().duration_since_unix_time().as_nanos();
        let dt = OffsetDateTime::from_unix_timestamp_nanos(
            nanos
                .try_into()
                .map_err(|e| Error::Time(format!("Convert nanos to i128 failed: {e}")))?,
        )
        .map_err(|e| Error::Time(format!("Instantiate Offset datetime: {e}")))?
        .to_offset(self.utc_offset);
        self.format.format(dt)
    }
    #[must_use]
    pub fn new(format: Format, offset: UtcOffset) -> Self {
        Self {
            format,
            utc_offset: offset,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Format {
    chunks: Vec<FormatChunk>,
}

#[cfg(feature = "config-file")]
fn from_pattern<'de, D: serde::de::Deserializer<'de>>(
    deserializer: D,
) -> core::result::Result<Format, D::Error> {
    let s: &'de str = serde::de::Deserialize::deserialize(deserializer)?;
    Format::new(s)
        .map_err(|e| D::Error::custom(format!("Failed to deserialize valid date pattern {e}")))
}

#[cfg(feature = "config-file")]
fn from_hms_tuple<'de, D: serde::de::Deserializer<'de>>(
    deserializer: D,
) -> core::result::Result<UtcOffset, D::Error> {
    let (h, m, s): (i8, i8, i8) = serde::de::Deserialize::deserialize(deserializer)?;
    UtcOffset::from_hms(h, m, s)
        .map_err(|d| serde::de::Error::custom(format!("Failed to parse utc-offset {d:?}")))
}

impl Format {
    pub fn format(&self, dt: OffsetDateTime) -> crate::error::Result<String> {
        let mut out = String::new();
        for chunk in &self.chunks {
            match chunk {
                FormatChunk::Value(v) => {
                    out.push_str(v);
                }
                FormatChunk::Token(t) => {
                    t.write_into(&dt, &mut out)
                        .map_err(|e| Error::Time(format!("Writing token into string {e}")))?;
                }
            }
        }
        Ok(out)
    }

    pub fn new(input: &str) -> crate::error::Result<Self> {
        let mut chunks = Vec::new();
        let mut cur_raw_chunk = String::new();
        let mut state = State::Ready;
        for ch in input.chars() {
            match state {
                State::Ready => {
                    if ch == '{' {
                        state = State::SeenStartBracket;
                    } else {
                        cur_raw_chunk.push(ch);
                    }
                }
                State::SeenStartBracket => {
                    if ch == '%' {
                        state = State::SeenStartPerc;
                    } else {
                        state = State::Ready;
                    }
                }
                State::SeenStartPerc => {
                    let t = match ch {
                        'Y' => Token::Year,
                        'M' => Token::Month,
                        'W' => Token::Week,
                        'D' => Token::Day,
                        'd' => Token::WeekDay,
                        'h' => Token::Hour,
                        'm' => Token::Minute,
                        's' => Token::Second,
                        _ => return Err(Error::Time(format!("Bad token {ch}"))),
                    };
                    if !cur_raw_chunk.is_empty() {
                        chunks.push(FormatChunk::Value(core::mem::take(&mut cur_raw_chunk)));
                    }
                    chunks.push(FormatChunk::Token(t));
                    state = State::SeenValue;
                }
                State::SeenValue => {
                    if ch != '%' {
                        return Err(Error::Time(format!("Expected end perc, got {ch}")));
                    }
                    state = State::SeenEndPerc;
                }
                State::SeenEndPerc => {
                    if ch != '}' {
                        return Err(Error::Time(format!("Expected end bracket, got {ch}")));
                    }
                    state = State::Ready;
                }
            }
        }
        Ok(Self { chunks })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum FormatChunk {
    Value(String),
    Token(Token),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Token {
    Year,
    Month,
    Week,
    WeekDay,
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Copy, Clone)]
enum State {
    Ready,
    SeenStartBracket,
    SeenStartPerc,
    SeenValue,
    SeenEndPerc,
}

impl Token {
    fn write_into<W>(self, dt: &OffsetDateTime, sink: &mut W) -> Result<(), core::fmt::Error>
    where
        W: core::fmt::Write,
    {
        match self {
            Token::Year => {
                sink.write_fmt(format_args!("{}", dt.year()))?;
            }
            Token::Month => match dt.month() {
                Month::January => sink.write_fmt(format_args!("Jan"))?,
                Month::February => sink.write_fmt(format_args!("Feb"))?,
                Month::March => sink.write_fmt(format_args!("Mar"))?,
                Month::April => sink.write_fmt(format_args!("Apr"))?,
                Month::May => sink.write_fmt(format_args!("May"))?,
                Month::June => sink.write_fmt(format_args!("Jun"))?,
                Month::July => sink.write_fmt(format_args!("Jul"))?,
                Month::August => sink.write_fmt(format_args!("Aul"))?,
                Month::September => sink.write_fmt(format_args!("Sep"))?,
                Month::October => sink.write_fmt(format_args!("Oct"))?,
                Month::November => sink.write_fmt(format_args!("Nov"))?,
                Month::December => sink.write_fmt(format_args!("Dec"))?,
            },
            Token::Week => sink.write_fmt(format_args!("{}", dt.iso_week()))?,
            Token::WeekDay => match dt.weekday() {
                Weekday::Monday => sink.write_fmt(format_args!("Mon"))?,
                Weekday::Tuesday => sink.write_fmt(format_args!("Tue"))?,
                Weekday::Wednesday => sink.write_fmt(format_args!("Wed"))?,
                Weekday::Thursday => sink.write_fmt(format_args!("Thu"))?,
                Weekday::Friday => sink.write_fmt(format_args!("Fri"))?,
                Weekday::Saturday => sink.write_fmt(format_args!("Sat"))?,
                Weekday::Sunday => sink.write_fmt(format_args!("Sun"))?,
            },
            Token::Day => sink.write_fmt(format_args!("{}", dt.day()))?,
            Token::Hour => {
                let t = dt.hour();
                if t < 10 {
                    sink.write_fmt(format_args!("0{t}"))?;
                } else {
                    sink.write_fmt(format_args!("{t}"))?;
                }
            }
            Token::Minute => {
                let t = dt.minute();
                if t < 10 {
                    sink.write_fmt(format_args!("0{t}"))?;
                } else {
                    sink.write_fmt(format_args!("{t}"))?;
                }
            }
            Token::Second => {
                let t = dt.second();
                if t < 10 {
                    sink.write_fmt(format_args!("0{t}"))?;
                } else {
                    sink.write_fmt(format_args!("{t}"))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_time() {
        let in_str = "Year {%Y%}, Month {%M%}, Week {%W%}, Day {%D%}, hour {%h%}, minute {%m%}, second {%s%}";
        let fmt = Format::new(in_str).unwrap();
        let dt = OffsetDateTime::from_unix_timestamp_nanos(1_666_551_103_791_951_912i128).unwrap();
        let expect = "Year 2022, Month Oct, Week 42, Day 23, hour 18, minute 51, second 43";
        assert_eq!(expect, fmt.format(dt).unwrap());
    }
}
