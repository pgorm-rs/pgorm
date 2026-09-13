//! Temporal text, in both directions.
//!
//! The wire canon is Python's spelling: a space between date and time, an
//! explicit `+00:00` on the offset-aware kind, and a fractional field that is
//! either absent or exactly six digits. jiff prints RFC 3339 — a `T`, a `Z`,
//! and a fraction trimmed to its significant digits — so the payload the
//! encoder writes is not the text the oracle compares.
//!
//! [`temporal_text`] is `wire.temporal_text` reimplemented: it moves native
//! text onto the canon. The parsers below run it, read the result, and then
//! re-render it through [`canonical`]. That last step is load-bearing rather
//! than decorative, because jiff's parsers are deliberately permissive — they
//! read a basic-format date, and they accept and silently discard an offset on
//! a civil type. Re-rendering is what turns every spelling the canon does not
//! admit back into a rejection.

use jiff::{
    Timestamp,
    civil::{Date, DateTime, Time},
    tz::TimeZone,
};

use crate::FormatError;

fn invalid() -> FormatError {
    FormatError::new("invalid or unrepresentable temporal payload")
}

/// Normalise native display text into the ISO spelling `wire.py` reads.
///
/// Accepts a `T` or space separator and a trailing `Z`, and pads or drops a
/// fractional-second field the way Python's microsecond-resolution parsers
/// require.
///
/// # Errors
///
/// Returns [`FormatError`] when the fraction carries more than six digits.
/// jiff itself retains nanoseconds, so this is the boundary that keeps a
/// precision Python cannot represent from crossing into the oracle as a
/// value the two sides would silently disagree about.
pub fn temporal_text(data: &str) -> Result<String, FormatError> {
    let mut value = data.replace('T', " ");
    if value.ends_with('Z') {
        value.truncate(value.len() - 1);
        value.push_str("+00:00");
    }
    let Some((start, end)) = fraction(&value) else {
        return Ok(value);
    };
    let digits = &value[start + 1..end];
    if digits.len() > 6 {
        return Err(FormatError::new(
            "temporal payload exceeds Python's microsecond precision",
        ));
    }
    let rendered = if digits.bytes().all(|byte| byte == b'0') {
        String::new()
    } else {
        format!(".{digits:0<6}")
    };
    Ok(format!("{}{rendered}{}", &value[..start], &value[end..]))
}

/// Read a `date` payload.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical ISO date.
pub fn parse_date(text: &str) -> Result<Date, FormatError> {
    let normalized = temporal_text(text)?;
    let value: Date = normalized.parse().map_err(|_| invalid())?;
    canonical(&normalized, &value.to_string())?;
    Ok(value)
}

/// Read a `time` payload, which carries no offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical ISO time of day,
/// or when it carries an offset.
pub fn parse_time(text: &str) -> Result<Time, FormatError> {
    let normalized = temporal_text(text)?;
    if split_offset(&normalized).1.is_some() {
        return Err(FormatError::new("time payload cannot have an offset"));
    }
    let value: Time = normalized.parse().map_err(|_| invalid())?;
    canonical(&normalized, &render_time(value))?;
    Ok(value)
}

/// Read a `datetime` payload — naive, and required to carry no offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical naive timestamp,
/// or when it carries an offset.
pub fn parse_naive_datetime(text: &str) -> Result<DateTime, FormatError> {
    let normalized = temporal_text(text)?;
    if split_offset(&normalized).1.is_some() {
        return Err(FormatError::new("datetime kind and timezone do not agree"));
    }
    let value: DateTime = normalized.parse().map_err(|_| invalid())?;
    canonical(&normalized, &render_naive(value))?;
    Ok(value)
}

/// Read a `datetime_utc` payload, which must carry a zero offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical timestamp, when it
/// carries no offset or an offset other than zero, or when the instant falls
/// outside the range jiff can hold.
pub fn parse_datetime_utc(text: &str) -> Result<Timestamp, FormatError> {
    let normalized = temporal_text(text)?;
    let (civil, offset) = split_offset(&normalized);
    match offset {
        None => return Err(FormatError::new("datetime kind and timezone do not agree")),
        Some("+00:00") => (),
        Some(_) => return Err(FormatError::new("UTC datetime requires a zero offset")),
    }
    let value: DateTime = civil.parse().map_err(|_| invalid())?;
    canonical(&normalized, &format!("{}+00:00", render_naive(value)))?;
    TimeZone::UTC.to_timestamp(value).map_err(|_| invalid())
}

fn canonical(normalized: &str, rendered: &str) -> Result<(), FormatError> {
    // ISO parsers accept and truncate extra precision; preserve it or reject it.
    if normalized == rendered {
        Ok(())
    } else {
        Err(FormatError::new(
            "temporal text is noncanonical or loses precision",
        ))
    }
}

/// The canon writes six fractional digits or none, where jiff's own `Display`
/// writes the fewest that round-trip, so the fraction is rendered by hand.
fn render_time(value: Time) -> String {
    let (hour, minute, second) = (value.hour(), value.minute(), value.second());
    let micros = value.subsec_nanosecond() / 1_000;
    if micros == 0 {
        format!("{hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{hour:02}:{minute:02}:{second:02}.{micros:06}")
    }
}

fn render_naive(value: DateTime) -> String {
    format!("{} {}", value.date(), render_time(value.time()))
}

/// Peel a trailing `+HH:MM` / `-HH:MM`, so a kind can require one or refuse it
/// rather than leaving the question to a parser that discards what it finds.
fn split_offset(value: &str) -> (&str, Option<&str>) {
    let bytes = value.as_bytes();
    let Some(start) = bytes.len().checked_sub(6) else {
        return (value, None);
    };
    let tail = &bytes[start..];
    let shaped = matches!(tail[0], b'+' | b'-')
        && tail[1].is_ascii_digit()
        && tail[2].is_ascii_digit()
        && tail[3] == b':'
        && tail[4].is_ascii_digit()
        && tail[5].is_ascii_digit();
    if shaped {
        (&value[..start], Some(&value[start..]))
    } else {
        (value, None)
    }
}

/// `re.search(r"\.(\d+)", value)`: the first dot that is actually followed by
/// digits, as byte bounds over the dot and the digit run.
fn fraction(value: &str) -> Option<(usize, usize)> {
    let bytes = value.as_bytes();
    for (start, byte) in bytes.iter().enumerate() {
        if *byte != b'.' {
            continue;
        }
        let mut end = start + 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > start + 1 {
            return Some((start, end));
        }
    }
    None
}
