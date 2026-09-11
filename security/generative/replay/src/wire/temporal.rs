//! Temporal text, in both directions.
//!
//! chrono's `Display` and its `FromStr` disagree: `NaiveDateTime` *prints* a
//! space between date and time and *parses* only a `T`, and `DateTime<Utc>`
//! prints a trailing `" UTC"` that no ISO parser accepts. `wire.temporal_text`
//! is the Python side's reconciliation of the two — it normalises chrono's
//! spelling into something `datetime.fromisoformat` reads, and the encoder
//! emits chrono's spelling verbatim on the strength of it.
//!
//! [`temporal_text`] is that normalisation, reimplemented; the parsers below
//! run it and then read the result with an explicit format, so `"…".parse()`
//! never gets the chance to fail on the separator.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};

use crate::FormatError;

const DATE: &str = "%Y-%m-%d";
const TIME: &str = "%H:%M:%S";
const MICROS: &str = "%H:%M:%S%.6f";
const READ_TIME: &str = "%H:%M:%S%.f";

/// Normalise native chrono display text into the ISO spelling `wire.py` reads.
///
/// Accepts `" UTC"` and `" +HH:MM"` suffixes, a `T` or space separator and a
/// trailing `Z`, and pads or drops a fractional-second field the way Python's
/// microsecond-resolution parsers require.
///
/// # Errors
///
/// Returns [`FormatError`] when the fraction carries more than six digits,
/// which Python cannot represent and therefore cannot compare.
pub fn temporal_text(data: &str) -> Result<String, FormatError> {
    let mut value = match data.strip_suffix(" UTC") {
        Some(head) => format!("{head}+00:00"),
        None => data.to_owned(),
    };
    value = strip_offset_space(&value);
    let zulu = value.ends_with('Z');
    value = value.replace('T', " ");
    if zulu {
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
pub fn parse_date(text: &str) -> Result<NaiveDate, FormatError> {
    let normalized = temporal_text(text)?;
    let value = NaiveDate::parse_from_str(&normalized, DATE)
        .map_err(|_| FormatError::new("invalid or unrepresentable temporal payload"))?;
    canonical(&normalized, &format!("{}", value.format(DATE)))?;
    Ok(value)
}

/// Read a `time` payload.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical ISO time of day.
pub fn parse_time(text: &str) -> Result<NaiveTime, FormatError> {
    let normalized = temporal_text(text)?;
    let value = NaiveTime::parse_from_str(&normalized, READ_TIME)
        .map_err(|_| FormatError::new("invalid or unrepresentable temporal payload"))?;
    canonical(&normalized, &render_time(&value))?;
    Ok(value)
}

/// Read a `datetime` payload — naive, and required to carry no offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical naive timestamp.
pub fn parse_naive_datetime(text: &str) -> Result<NaiveDateTime, FormatError> {
    let normalized = temporal_text(text)?;
    let format = format!("{DATE} {READ_TIME}");
    let value = NaiveDateTime::parse_from_str(&normalized, &format)
        .map_err(|_| FormatError::new("invalid or unrepresentable temporal payload"))?;
    canonical(&normalized, &render_naive(&value))?;
    Ok(value)
}

/// Read a `datetime_fixed` payload, retaining its offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical offset timestamp.
pub fn parse_datetime_fixed(text: &str) -> Result<DateTime<FixedOffset>, FormatError> {
    let normalized = temporal_text(text)?;
    let format = format!("{DATE} {READ_TIME}%:z");
    let value = DateTime::parse_from_str(&normalized, &format)
        .map_err(|_| FormatError::new("invalid or unrepresentable temporal payload"))?;
    canonical(&normalized, &render_offset(&value))?;
    Ok(value)
}

/// Read a `datetime_utc` payload, which must carry a zero offset.
///
/// # Errors
///
/// Returns [`FormatError`] when the text is not a canonical timestamp, or when
/// it carries an offset other than zero.
pub fn parse_datetime_utc(text: &str) -> Result<DateTime<Utc>, FormatError> {
    let value = parse_datetime_fixed(text)?;
    if value.offset().local_minus_utc() != 0 {
        return Err(FormatError::new("UTC datetime requires a zero offset"));
    }
    Ok(value.with_timezone(&Utc))
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

fn render_time(value: &NaiveTime) -> String {
    if value.nanosecond() == 0 {
        format!("{}", value.format(TIME))
    } else {
        format!("{}", value.format(MICROS))
    }
}

fn render_naive(value: &NaiveDateTime) -> String {
    format!("{} {}", value.format(DATE), render_time(&value.time()))
}

fn render_offset(value: &DateTime<FixedOffset>) -> String {
    format!("{}{}", render_naive(&value.naive_local()), value.offset())
}

/// `re.sub(r" ([+-][0-9]{2}:[0-9]{2})$", r"\1", value)`: chrono separates an
/// offset from the timestamp with a space where ISO 8601 does not.
fn strip_offset_space(value: &str) -> String {
    let bytes = value.as_bytes();
    let Some(start) = bytes.len().checked_sub(7) else {
        return value.to_owned();
    };
    let tail = &bytes[start..];
    let shaped = tail[0] == b' '
        && matches!(tail[1], b'+' | b'-')
        && tail[2].is_ascii_digit()
        && tail[3].is_ascii_digit()
        && tail[4] == b':'
        && tail[5].is_ascii_digit()
        && tail[6].is_ascii_digit();
    if shaped {
        format!("{}{}", &value[..start], &value[start + 1..])
    } else {
        value.to_owned()
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
