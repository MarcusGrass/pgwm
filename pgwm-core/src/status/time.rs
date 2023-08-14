use alloc::format;
use alloc::string::String;

use time::{Month, OffsetDateTime, UtcOffset, Weekday};
use tiny_std::time::SystemTime;

use crate::error::Error;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ClockFormatter {
    format: Format,
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
    pub const fn new(format: Format, offset: UtcOffset) -> Self {
        Self {
            format,
            utc_offset: offset,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Format {
    chunks: &'static [FormatChunk],
}

impl Format {
    pub fn format(&self, dt: OffsetDateTime) -> crate::error::Result<String> {
        let mut out = String::new();
        for chunk in self.chunks {
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

    #[must_use]
    pub const fn new(chunks: &'static [FormatChunk]) -> Self {
        Self { chunks }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FormatChunk {
    Value(&'static str),
    Token(Token),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Token {
    Year,
    Month,
    Week,
    WeekDay,
    Day,
    Hour,
    Minute,
    Second,
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
                Month::August => sink.write_fmt(format_args!("Aug"))?,
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
        let fmt = crate::status::time::Format::new(&[
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Year),
            crate::status::time::FormatChunk::Value(" "),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Month),
            crate::status::time::FormatChunk::Value(" "),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Day),
            crate::status::time::FormatChunk::Value(" w"),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Week),
            crate::status::time::FormatChunk::Value(" "),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Hour),
            crate::status::time::FormatChunk::Value(":"),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Minute),
            crate::status::time::FormatChunk::Value(":"),
            crate::status::time::FormatChunk::Token(crate::status::time::Token::Second),
        ]);
        let dt = OffsetDateTime::from_unix_timestamp_nanos(1_666_551_103_791_951_912i128).unwrap();
        let expect = "2022 Oct 23 w42 18:51:43";
        assert_eq!(expect, fmt.format(dt).unwrap());
    }
}
