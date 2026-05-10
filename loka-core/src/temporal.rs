//! Temporal literal type for ontochronological indexing.
//!
//! The `loka:temporal` datatype encodes a timestamp with precision.
//! Precision is derived from the input format — "1847" is year-level,
//! "1847-03-15" is day-level.
//!
//! # Inline encoding
//!
//! Temporal values are encoded inline in 56-bit TermId payloads:
//! - Bits 55–8: 48-bit signed timestamp (seconds since Unix epoch)
//! - Bits 7–4: 4-bit precision level
//! - Bits 3–0: reserved (zero)
//!
//! 48-bit seconds gives ±4.4 million years from epoch — sufficient
//! for any historical or future use case.

use crate::error::{CoreError, Result};
use crate::id::TermId;

// ---------------------------------------------------------------------------
// Precision
// ---------------------------------------------------------------------------

/// Temporal precision levels, ordered from finest to coarsest.
///
/// Stored in 4 bits (values 0–9). The ordering is finest-first so that
/// finer precisions have smaller discriminant values.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalPrecision {
    Second = 0,
    Minute = 1,
    Hour = 2,
    Day = 3,
    Month = 4,
    Year = 5,
    Decade = 6,
    Century = 7,
    Millennium = 8,
}

impl TemporalPrecision {
    /// Convert a raw 4-bit value back to a precision level.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Second),
            1 => Some(Self::Minute),
            2 => Some(Self::Hour),
            3 => Some(Self::Day),
            4 => Some(Self::Month),
            5 => Some(Self::Year),
            6 => Some(Self::Decade),
            7 => Some(Self::Century),
            8 => Some(Self::Millennium),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Temporal signifier (which temporal predicate produced this entry)
// ---------------------------------------------------------------------------

/// Which temporal predicate an index entry came from.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemporalSignifier {
    /// `loka:assertedAt` — point attestation.
    AssertedAt = 0,
    /// `loka:validFrom` — interval start.
    ValidFrom = 1,
    /// `loka:validTo` — interval end.
    ValidTo = 2,
}

impl TemporalSignifier {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::AssertedAt),
            1 => Some(Self::ValidFrom),
            2 => Some(Self::ValidTo),
            _ => None,
        }
    }
}

/// Well-known IRI strings for the three temporal predicates.
pub const PREDICATE_ASSERTED_AT: &str = "https://loka.dev/ns/assertedAt";
pub const PREDICATE_VALID_FROM: &str = "https://loka.dev/ns/validFrom";
pub const PREDICATE_VALID_TO: &str = "https://loka.dev/ns/validTo";

/// The datatype IRI for temporal literals.
pub const DATATYPE_TEMPORAL: &str = "https://loka.dev/ns/temporal";

// ---------------------------------------------------------------------------
// Temporal containment
// ---------------------------------------------------------------------------

/// The result of evaluating whether a triple is valid at a query time T.
///
/// Three-valued temporal logic: a triple is either certainly valid (Definite),
/// probably valid with measurable uncertainty (Open), certainly not valid
/// (Outside), or timelessly true (Atemporal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalContainment {
    /// T is within a closed interval. The triple is certainly valid at T.
    Definite,
    /// T is on the unbounded side of a half-open interval.
    /// `distance` is the number of axis units (seconds for UTC) from the
    /// nearest known temporal endpoint. Larger distance = less certainty.
    Open { distance: i64 },
    /// T is outside all temporal intervals. The triple is not valid at T.
    Outside,
    /// The triple has no temporal annotations. It is valid at all times.
    Atemporal,
}

impl TemporalContainment {
    /// Ordering key for result ranking. Lower = higher priority.
    /// Atemporal(0) > Definite(1) > Open(2+distance) > Outside(i64::MAX).
    pub fn rank(&self) -> i64 {
        match self {
            Self::Atemporal => 0,
            Self::Definite => 1,
            Self::Open { distance } => 2 + *distance,
            Self::Outside => i64::MAX,
        }
    }

    /// Whether this triple should be included in temporal query results.
    /// Outside triples are always excluded.
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Outside)
    }

    /// Whether this triple should be included given a maximum open distance.
    /// Definite and Atemporal always pass. Open passes if distance ≤ max.
    /// Outside never passes.
    pub fn is_visible_within(&self, max_distance: i64) -> bool {
        match self {
            Self::Atemporal | Self::Definite => true,
            Self::Open { distance } => *distance <= max_distance,
            Self::Outside => false,
        }
    }
}

/// Temporal annotations collected for a single triple, used to evaluate
/// containment at a query time.
///
/// Built by gathering TSPO index entries for a triple's (S, P, O).
#[derive(Debug, Clone, Default)]
pub struct TemporalAnnotations {
    /// All `loka:assertedAt` timestamps for this triple.
    pub asserted_at: Vec<i64>,
    /// All `loka:validFrom` timestamps (interval start points).
    pub valid_from: Vec<i64>,
    /// All `loka:validTo` timestamps (interval end points).
    pub valid_to: Vec<i64>,
}

impl TemporalAnnotations {
    /// Returns true if this triple has no temporal annotations at all.
    pub fn is_atemporal(&self) -> bool {
        self.asserted_at.is_empty() && self.valid_from.is_empty() && self.valid_to.is_empty()
    }

    /// Evaluate the temporal containment of this triple at query time `t`.
    ///
    /// The evaluation follows this precedence:
    /// 1. If no annotations exist → Atemporal
    /// 2. Check closed intervals (validFrom/validTo pairs, or validFrom/assertedAt)
    /// 3. Check open intervals (validFrom without validTo, or vice versa)
    /// 4. Check point attestations (assertedAt alone)
    /// 5. Otherwise → Outside
    pub fn containment_at(&self, t: i64) -> TemporalContainment {
        if self.is_atemporal() {
            return TemporalContainment::Atemporal;
        }

        // --- Closed intervals: pair validFrom[i] with validTo[i] ---
        // Sort both lists and pair them positionally. Unpaired entries
        // become half-open intervals handled below.
        let mut starts: Vec<i64> = self.valid_from.clone();
        let mut ends: Vec<i64> = self.valid_to.clone();
        starts.sort();
        ends.sort();

        let paired = starts.len().min(ends.len());

        // Check closed intervals (paired start/end)
        for i in 0..paired {
            if starts[i] <= t && t <= ends[i] {
                return TemporalContainment::Definite;
            }
        }

        // Check validFrom + assertedAt as bounded interval
        // (any start that wasn't paired with an end can use assertedAt)
        if !self.asserted_at.is_empty() {
            let max_asserted = *self.asserted_at.iter().max().unwrap();
            for &start in starts.iter().skip(paired) {
                if start <= t && t <= max_asserted {
                    return TemporalContainment::Definite;
                }
            }
        }

        // Exact assertedAt match is definite
        for &a in &self.asserted_at {
            if a == t {
                return TemporalContainment::Definite;
            }
        }

        // --- Outside checks on closed intervals ---
        // If ALL intervals are closed and T is outside all of them, it's Outside.
        // But if there are open intervals, we check those first.

        // --- Open intervals: unpaired starts (no end) ---
        let mut min_distance = i64::MAX;

        for &start in starts.iter().skip(paired) {
            if t >= start {
                // T is past the start with no known end
                let d = t - start;
                min_distance = min_distance.min(d);
            }
            // t < start → Outside for this interval (hasn't started)
        }

        // --- Open intervals: unpaired ends (no start) ---
        for &end in ends.iter().skip(paired) {
            if t <= end {
                // T is before the end with no known start
                let d = end - t;
                min_distance = min_distance.min(d);
            }
            // t > end → Outside for this interval (already ended)
        }

        // --- Assertion time as open point ---
        // If assertedAt exists but no validFrom/validTo, distance from
        // the assertion point determines openness.
        if starts.is_empty() && ends.is_empty() && !self.asserted_at.is_empty() {
            for &a in &self.asserted_at {
                let d = (t - a).abs();
                if d > 0 {
                    min_distance = min_distance.min(d);
                }
            }
        }

        if min_distance < i64::MAX {
            return TemporalContainment::Open {
                distance: min_distance,
            };
        }

        TemporalContainment::Outside
    }
}

// ---------------------------------------------------------------------------
// TemporalValue
// ---------------------------------------------------------------------------

/// A temporal value: timestamp + precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemporalValue {
    /// Seconds since Unix epoch (1970-01-01T00:00:00Z).
    /// Negative for dates before epoch.
    pub timestamp: i64,
    /// How precise this timestamp is.
    pub precision: TemporalPrecision,
}

impl TemporalValue {
    /// Create a new temporal value.
    pub fn new(timestamp: i64, precision: TemporalPrecision) -> Self {
        Self {
            timestamp,
            precision,
        }
    }

    /// Returns the half-open interval `[start, end)` in seconds that this
    /// value covers, based on its precision.
    ///
    /// For example, year 1847 covers `[start_of_1847, start_of_1848)`.
    pub fn interval(&self) -> (i64, i64) {
        match self.precision {
            TemporalPrecision::Second => (self.timestamp, self.timestamp + 1),
            TemporalPrecision::Minute => (self.timestamp, self.timestamp + 60),
            TemporalPrecision::Hour => (self.timestamp, self.timestamp + 3600),
            TemporalPrecision::Day => (self.timestamp, self.timestamp + 86400),
            TemporalPrecision::Month => {
                let (y, m, _) = civil_from_seconds(self.timestamp);
                let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                let end = seconds_from_civil(ny, nm, 1);
                (self.timestamp, end)
            }
            TemporalPrecision::Year => {
                let (y, _, _) = civil_from_seconds(self.timestamp);
                let end = seconds_from_civil(y + 1, 1, 1);
                (self.timestamp, end)
            }
            TemporalPrecision::Decade => {
                let (y, _, _) = civil_from_seconds(self.timestamp);
                let end = seconds_from_civil(y + 10, 1, 1);
                (self.timestamp, end)
            }
            TemporalPrecision::Century => {
                let (y, _, _) = civil_from_seconds(self.timestamp);
                let end = seconds_from_civil(y + 100, 1, 1);
                (self.timestamp, end)
            }
            TemporalPrecision::Millennium => {
                let (y, _, _) = civil_from_seconds(self.timestamp);
                let end = seconds_from_civil(y + 1000, 1, 1);
                (self.timestamp, end)
            }
        }
    }

    /// Check whether a given timestamp (seconds since epoch) falls within
    /// this value's precision interval.
    pub fn contains(&self, t: i64) -> bool {
        let (lo, hi) = self.interval();
        lo <= t && t < hi
    }
}

// ---------------------------------------------------------------------------
// Inline encoding  (48-bit timestamp + 4-bit precision in 56-bit payload)
// ---------------------------------------------------------------------------

const INLINE_BIT: u64 = 1 << 63;
const TYPE_TAG_SHIFT: u32 = 56;
/// Type tag for temporal literals (must match `InlineType::Temporal` in id.rs).
const TEMPORAL_TAG: u64 = 0x03;
const PAYLOAD_MASK: u64 = (1u64 << 56) - 1;

// Within the 56-bit payload:
//   bits 55–8  (48 bits): signed timestamp in seconds
//   bits  7–4  ( 4 bits): precision
//   bits  3–0  ( 4 bits): reserved

const TS_SHIFT: u32 = 8;
const TS_MASK_48: u64 = (1u64 << 48) - 1;
const PREC_SHIFT: u32 = 4;
const PREC_MASK: u64 = 0x0F;

/// Encode a `TemporalValue` as an inline TermId.
///
/// Returns `None` if the timestamp doesn't fit in 48 bits signed
/// (roughly ±4.4 million years from epoch).
pub fn inline_temporal(val: &TemporalValue) -> Option<TermId> {
    let ts = val.timestamp;
    // 48-bit signed range: -(2^47) .. (2^47 - 1)
    if !(-(1i64 << 47)..(1i64 << 47)).contains(&ts) {
        return None;
    }
    let ts_bits = (ts as u64) & TS_MASK_48;
    let prec_bits = (val.precision as u64) & PREC_MASK;
    let payload = (ts_bits << TS_SHIFT) | (prec_bits << PREC_SHIFT);
    Some(INLINE_BIT | (TEMPORAL_TAG << TYPE_TAG_SHIFT) | payload)
}

/// Decode an inline TermId back to a `TemporalValue`.
///
/// Returns `None` if the TermId is not an inline temporal literal.
pub fn decode_inline_temporal(id: TermId) -> Option<TemporalValue> {
    if id & INLINE_BIT == 0 {
        return None;
    }
    let tag = (id >> TYPE_TAG_SHIFT) & 0x7F;
    if tag != TEMPORAL_TAG {
        return None;
    }
    let payload = id & PAYLOAD_MASK;
    let ts_raw = (payload >> TS_SHIFT) & TS_MASK_48;
    // Sign-extend from 48 bits
    let ts = if ts_raw & (1 << 47) != 0 {
        (ts_raw | !TS_MASK_48) as i64
    } else {
        ts_raw as i64
    };
    let prec_raw = ((payload >> PREC_SHIFT) & PREC_MASK) as u8;
    let precision = TemporalPrecision::from_u8(prec_raw)?;
    Some(TemporalValue {
        timestamp: ts,
        precision,
    })
}

// ---------------------------------------------------------------------------
// TSPO key encoding  (33 bytes)
// ---------------------------------------------------------------------------

/// Encode a TSPO index key (33 bytes).
///
/// Layout: `[signifier:1 | timestamp:8 | subject:8 | predicate:8 | object:8]`
///
/// The timestamp uses sign-bit-flip encoding so that signed i64 values
/// sort correctly as unsigned bytes.
pub fn tspo_key(
    signifier: TemporalSignifier,
    timestamp: i64,
    subject: u64,
    predicate: u64,
    object: u64,
) -> [u8; 33] {
    let mut key = [0u8; 33];
    key[0] = signifier as u8;
    key[1..9].copy_from_slice(&timestamp_to_sortable(timestamp));
    key[9..17].copy_from_slice(&subject.to_be_bytes());
    key[17..25].copy_from_slice(&predicate.to_be_bytes());
    key[25..33].copy_from_slice(&object.to_be_bytes());
    key
}

/// Decode a 33-byte TSPO key.
pub fn decode_tspo_key(key: &[u8; 33]) -> (TemporalSignifier, i64, u64, u64, u64) {
    let signifier = TemporalSignifier::from_u8(key[0]).unwrap_or(TemporalSignifier::AssertedAt);
    let ts = sortable_to_timestamp(key[1..9].try_into().unwrap());
    let subject = u64::from_be_bytes(key[9..17].try_into().unwrap());
    let predicate = u64::from_be_bytes(key[17..25].try_into().unwrap());
    let object = u64::from_be_bytes(key[25..33].try_into().unwrap());
    (signifier, ts, subject, predicate, object)
}

/// Convert signed i64 to 8 bytes that sort correctly as unsigned.
///
/// XOR the sign bit so negative values sort before positive.
fn timestamp_to_sortable(ts: i64) -> [u8; 8] {
    let mut bytes = ts.to_be_bytes();
    bytes[0] ^= 0x80;
    bytes
}

/// Reverse the sign-bit flip.
fn sortable_to_timestamp(mut bytes: [u8; 8]) -> i64 {
    bytes[0] ^= 0x80;
    i64::from_be_bytes(bytes)
}

// ---------------------------------------------------------------------------
// Parsing  ("1847", "1847-03", "1847-03-15", "1847-03-15T09:32:00", etc.)
// ---------------------------------------------------------------------------

/// Parse a temporal literal string into a `TemporalValue`.
///
/// Accepted formats (precision derived from format):
/// - `"YYYY"` → Year
/// - `"YYYY-MM"` → Month
/// - `"YYYY-MM-DD"` → Day
/// - `"YYYY-MM-DDTHH"` → Hour
/// - `"YYYY-MM-DDTHH:MM"` → Minute
/// - `"YYYY-MM-DDTHH:MM:SS"` → Second
///
/// Negative years for BCE dates: `"-0500"` = 501 BCE.
pub fn parse_temporal(s: &str) -> Result<TemporalValue> {
    let s = s.trim();
    if s.is_empty() {
        return Err(CoreError::InvalidTemporal("empty string".into()));
    }

    // Split off optional negative sign
    let (negative, rest) = if let Some(stripped) = s.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, s)
    };

    // Split at 'T' for date/time parts
    let (date_part, time_part) = match rest.find('T') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    // Parse date components
    let date_parts: Vec<&str> = date_part.split('-').collect();
    let year: i32 = date_parts
        .first()
        .ok_or_else(|| CoreError::InvalidTemporal(s.into()))?
        .parse()
        .map_err(|_| CoreError::InvalidTemporal(s.into()))?;
    let year = if negative { -year } else { year };

    let month: u32 = if date_parts.len() > 1 {
        date_parts[1]
            .parse()
            .map_err(|_| CoreError::InvalidTemporal(s.into()))?
    } else {
        0 // signals: no month provided
    };

    let day: u32 = if date_parts.len() > 2 {
        date_parts[2]
            .parse()
            .map_err(|_| CoreError::InvalidTemporal(s.into()))?
    } else {
        0
    };

    // Parse time components
    let (hour, minute, second, has_time) = if let Some(tp) = time_part {
        let time_parts: Vec<&str> = tp.split(':').collect();
        let h: u32 = time_parts
            .first()
            .unwrap_or(&"0")
            .parse()
            .map_err(|_| CoreError::InvalidTemporal(s.into()))?;
        let m: u32 = if time_parts.len() > 1 {
            time_parts[1]
                .parse()
                .map_err(|_| CoreError::InvalidTemporal(s.into()))?
        } else {
            0
        };
        let sec: u32 = if time_parts.len() > 2 {
            // Strip trailing 'Z' if present
            let sec_str = time_parts[2].trim_end_matches('Z');
            // Strip fractional seconds
            let sec_str = sec_str.split('.').next().unwrap_or(sec_str);
            sec_str
                .parse()
                .map_err(|_| CoreError::InvalidTemporal(s.into()))?
        } else {
            0
        };
        (h, m, sec, true)
    } else {
        (0, 0, 0, false)
    };

    // Determine precision
    let precision = if has_time {
        let tp = time_part.unwrap();
        let colon_count = tp.chars().filter(|&c| c == ':').count();
        match colon_count {
            0 => TemporalPrecision::Hour,
            1 => TemporalPrecision::Minute,
            _ => TemporalPrecision::Second,
        }
    } else {
        match date_parts.len() {
            1 => TemporalPrecision::Year,
            2 => TemporalPrecision::Month,
            _ => TemporalPrecision::Day,
        }
    };

    // Compute timestamp
    let eff_month = if month == 0 { 1 } else { month };
    let eff_day = if day == 0 { 1 } else { day };
    let ts = seconds_from_civil(year, eff_month, eff_day)
        + (hour as i64) * 3600
        + (minute as i64) * 60
        + (second as i64);

    Ok(TemporalValue {
        timestamp: ts,
        precision,
    })
}

// ---------------------------------------------------------------------------
// Calendar arithmetic (Howard Hinnant's algorithms, proleptic Gregorian)
// ---------------------------------------------------------------------------

/// Days from 1970-01-01 to the given civil date.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 {
        year as i64 - 1
    } else {
        year as i64
    };
    let m = if month <= 2 {
        month as i64 + 9
    } else {
        month as i64 - 3
    };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Seconds since epoch for the start of a given civil date.
fn seconds_from_civil(year: i32, month: u32, day: u32) -> i64 {
    days_from_civil(year, month, day) * 86400
}

/// Convert seconds since epoch back to (year, month, day).
fn civil_from_seconds(secs: i64) -> (i32, u32, u32) {
    // Divide rounding toward negative infinity
    let days = if secs >= 0 {
        secs / 86400
    } else {
        (secs - 86399) / 86400
    };
    civil_from_days(days)
}

/// Convert days since 1970-01-01 to (year, month, day).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Calendar math --

    #[test]
    fn epoch_is_correct() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(seconds_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn known_date() {
        // 2000-01-01 is day 10957
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
    }

    #[test]
    fn pre_epoch_date() {
        // 1969-12-31 is day -1
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }

    #[test]
    fn civil_roundtrip() {
        for &(y, m, d) in &[
            (1970, 1, 1),
            (2000, 2, 29), // leap
            (1847, 3, 15),
            (1, 1, 1),
            (-500, 6, 15),
        ] {
            let days = days_from_civil(y, m, d);
            let (y2, m2, d2) = civil_from_days(days);
            assert_eq!((y, m, d), (y2, m2, d2), "roundtrip failed for {y}-{m}-{d}");
        }
    }

    #[test]
    fn civil_from_seconds_roundtrip() {
        let ts = seconds_from_civil(1847, 3, 15);
        let (y, m, d) = civil_from_seconds(ts);
        assert_eq!((y, m, d), (1847, 3, 15));
    }

    // -- Parsing --

    #[test]
    fn parse_year() {
        let v = parse_temporal("1847").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Year);
        assert_eq!(v.timestamp, seconds_from_civil(1847, 1, 1));
    }

    #[test]
    fn parse_month() {
        let v = parse_temporal("1847-03").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Month);
        assert_eq!(v.timestamp, seconds_from_civil(1847, 3, 1));
    }

    #[test]
    fn parse_day() {
        let v = parse_temporal("1847-03-15").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Day);
        assert_eq!(v.timestamp, seconds_from_civil(1847, 3, 15));
    }

    #[test]
    fn parse_hour() {
        let v = parse_temporal("1847-03-15T09").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Hour);
        assert_eq!(v.timestamp, seconds_from_civil(1847, 3, 15) + 9 * 3600);
    }

    #[test]
    fn parse_minute() {
        let v = parse_temporal("2024-03-14T10:00").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Minute);
        assert_eq!(v.timestamp, seconds_from_civil(2024, 3, 14) + 10 * 3600);
    }

    #[test]
    fn parse_second() {
        let v = parse_temporal("2024-03-14T10:30:45").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Second);
        assert_eq!(
            v.timestamp,
            seconds_from_civil(2024, 3, 14) + 10 * 3600 + 30 * 60 + 45
        );
    }

    #[test]
    fn parse_negative_year() {
        let v = parse_temporal("-0500").unwrap();
        assert_eq!(v.precision, TemporalPrecision::Year);
        assert_eq!(v.timestamp, seconds_from_civil(-500, 1, 1));
    }

    #[test]
    fn parse_empty_fails() {
        assert!(parse_temporal("").is_err());
    }

    // -- Inline encoding --

    #[test]
    fn inline_roundtrip() {
        let cases = [
            TemporalValue::new(0, TemporalPrecision::Second),
            TemporalValue::new(seconds_from_civil(1847, 3, 15), TemporalPrecision::Day),
            TemporalValue::new(
                seconds_from_civil(2024, 6, 1) + 10 * 3600,
                TemporalPrecision::Hour,
            ),
            TemporalValue::new(seconds_from_civil(-500, 1, 1), TemporalPrecision::Year),
        ];
        for val in &cases {
            let id = inline_temporal(val).expect("should encode");
            let decoded = decode_inline_temporal(id).expect("should decode");
            assert_eq!(
                val.timestamp, decoded.timestamp,
                "timestamp mismatch for {val:?}"
            );
            assert_eq!(
                val.precision, decoded.precision,
                "precision mismatch for {val:?}"
            );
        }
    }

    #[test]
    fn inline_out_of_range() {
        // Beyond 48-bit signed range
        let huge = TemporalValue::new(1i64 << 47, TemporalPrecision::Second);
        assert!(inline_temporal(&huge).is_none());
    }

    // -- TSPO key encoding --

    #[test]
    fn tspo_roundtrip() {
        let key = tspo_key(TemporalSignifier::ValidFrom, -86400, 100, 200, 300);
        let (sig, ts, s, p, o) = decode_tspo_key(&key);
        assert_eq!(sig, TemporalSignifier::ValidFrom);
        assert_eq!(ts, -86400);
        assert_eq!((s, p, o), (100, 200, 300));
    }

    #[test]
    fn tspo_sort_order() {
        // Earlier timestamps should sort before later ones within same signifier
        let k1 = tspo_key(TemporalSignifier::ValidFrom, -1000, 1, 1, 1);
        let k2 = tspo_key(TemporalSignifier::ValidFrom, 0, 1, 1, 1);
        let k3 = tspo_key(TemporalSignifier::ValidFrom, 1000, 1, 1, 1);
        assert!(k1 < k2);
        assert!(k2 < k3);

        // AssertedAt entries sort before ValidFrom entries (signifier 0 < 1)
        let ka = tspo_key(TemporalSignifier::AssertedAt, 0, 1, 1, 1);
        let kv = tspo_key(TemporalSignifier::ValidFrom, 0, 1, 1, 1);
        assert!(ka < kv);
    }

    // -- Interval --

    #[test]
    fn year_interval() {
        let v = parse_temporal("2024").unwrap();
        let (lo, hi) = v.interval();
        assert_eq!(lo, seconds_from_civil(2024, 1, 1));
        assert_eq!(hi, seconds_from_civil(2025, 1, 1));
    }

    #[test]
    fn month_interval() {
        let v = parse_temporal("2024-02").unwrap();
        let (lo, hi) = v.interval();
        assert_eq!(lo, seconds_from_civil(2024, 2, 1));
        assert_eq!(hi, seconds_from_civil(2024, 3, 1)); // Feb in leap year
    }

    #[test]
    fn day_interval() {
        let v = parse_temporal("2024-03-14").unwrap();
        let (lo, hi) = v.interval();
        assert_eq!(lo, seconds_from_civil(2024, 3, 14));
        assert_eq!(hi, seconds_from_civil(2024, 3, 14) + 86400);
    }

    #[test]
    fn contains_check() {
        let v = parse_temporal("2024").unwrap();
        let mid = seconds_from_civil(2024, 6, 15);
        assert!(v.contains(mid));
        let before = seconds_from_civil(2023, 12, 31);
        assert!(!v.contains(before));
        let after = seconds_from_civil(2025, 1, 1);
        assert!(!v.contains(after));
    }

    // -- Temporal containment --

    #[test]
    fn containment_atemporal() {
        let ann = TemporalAnnotations::default();
        assert_eq!(ann.containment_at(0), TemporalContainment::Atemporal);
        assert!(ann.containment_at(0).is_visible());
    }

    #[test]
    fn containment_closed_interval() {
        // Napoleon as Emperor: 1804-05-18 to 1814-04-11
        let start = seconds_from_civil(1804, 5, 18);
        let end = seconds_from_civil(1814, 4, 11);
        let ann = TemporalAnnotations {
            valid_from: vec![start],
            valid_to: vec![end],
            ..Default::default()
        };

        // Inside → Definite
        let mid = seconds_from_civil(1810, 1, 1);
        assert_eq!(ann.containment_at(mid), TemporalContainment::Definite);

        // At boundaries → Definite
        assert_eq!(ann.containment_at(start), TemporalContainment::Definite);
        assert_eq!(ann.containment_at(end), TemporalContainment::Definite);

        // Before start → Outside
        let before = seconds_from_civil(1800, 1, 1);
        assert_eq!(ann.containment_at(before), TemporalContainment::Outside);

        // After end → Outside
        let after = seconds_from_civil(1820, 1, 1);
        assert_eq!(ann.containment_at(after), TemporalContainment::Outside);
    }

    #[test]
    fn containment_open_interval_no_end() {
        // Alice works at Acme from 2023-01-15, no end date
        let start = seconds_from_civil(2023, 1, 15);
        let ann = TemporalAnnotations {
            valid_from: vec![start],
            ..Default::default()
        };

        // Query in 2024 → Open, distance ~1 year
        let query = seconds_from_civil(2024, 1, 15);
        match ann.containment_at(query) {
            TemporalContainment::Open { distance } => {
                assert_eq!(distance, query - start);
                assert!(distance > 0);
            }
            other => panic!("expected Open, got {other:?}"),
        }

        // Before start → Outside
        let before = seconds_from_civil(2022, 1, 1);
        assert_eq!(ann.containment_at(before), TemporalContainment::Outside);
    }

    #[test]
    fn containment_open_interval_no_start() {
        // Building existed until 1950, no known start
        let end = seconds_from_civil(1950, 1, 1);
        let ann = TemporalAnnotations {
            valid_to: vec![end],
            ..Default::default()
        };

        // Query in 1900 → Open, distance = 50 years
        let query = seconds_from_civil(1900, 1, 1);
        match ann.containment_at(query) {
            TemporalContainment::Open { distance } => {
                assert_eq!(distance, end - query);
            }
            other => panic!("expected Open, got {other:?}"),
        }

        // After end → Outside
        let after = seconds_from_civil(1960, 1, 1);
        assert_eq!(ann.containment_at(after), TemporalContainment::Outside);
    }

    #[test]
    fn containment_assertion_only_exact() {
        // Building observed in 1847
        let asserted = seconds_from_civil(1847, 1, 1);
        let ann = TemporalAnnotations {
            asserted_at: vec![asserted],
            ..Default::default()
        };

        // Exact match → Definite
        assert_eq!(ann.containment_at(asserted), TemporalContainment::Definite);

        // Nearby → Open with distance
        let nearby = seconds_from_civil(1848, 1, 1);
        match ann.containment_at(nearby) {
            TemporalContainment::Open { distance } => {
                assert_eq!(distance, (nearby - asserted).abs());
            }
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn containment_validfrom_plus_assertedat() {
        // Triple started at T1, asserted at T2. [T1, T2] is closed.
        let start = seconds_from_civil(2020, 1, 1);
        let asserted = seconds_from_civil(2023, 6, 1);
        let ann = TemporalAnnotations {
            valid_from: vec![start],
            asserted_at: vec![asserted],
            ..Default::default()
        };

        // Between start and assertedAt → Definite
        let mid = seconds_from_civil(2022, 1, 1);
        assert_eq!(ann.containment_at(mid), TemporalContainment::Definite);

        // After assertedAt → Open (past the last known point)
        let after = seconds_from_civil(2025, 1, 1);
        match ann.containment_at(after) {
            TemporalContainment::Open { distance } => {
                assert!(distance > 0);
            }
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn containment_multiple_intervals() {
        // Person held title in two periods: 2018-2020, 2022-2024
        let ann = TemporalAnnotations {
            valid_from: vec![
                seconds_from_civil(2018, 1, 1),
                seconds_from_civil(2022, 3, 1),
            ],
            valid_to: vec![
                seconds_from_civil(2020, 6, 30),
                seconds_from_civil(2024, 1, 15),
            ],
            ..Default::default()
        };

        // In first interval → Definite
        assert_eq!(
            ann.containment_at(seconds_from_civil(2019, 6, 1)),
            TemporalContainment::Definite
        );

        // In second interval → Definite
        assert_eq!(
            ann.containment_at(seconds_from_civil(2023, 1, 1)),
            TemporalContainment::Definite
        );

        // In gap between → Outside
        assert_eq!(
            ann.containment_at(seconds_from_civil(2021, 6, 1)),
            TemporalContainment::Outside
        );

        // Before all → Outside
        assert_eq!(
            ann.containment_at(seconds_from_civil(2017, 1, 1)),
            TemporalContainment::Outside
        );
    }

    #[test]
    fn containment_ranking() {
        assert!(TemporalContainment::Atemporal.rank() < TemporalContainment::Definite.rank());
        assert!(
            TemporalContainment::Definite.rank() < TemporalContainment::Open { distance: 1 }.rank()
        );
        assert!(
            TemporalContainment::Open { distance: 1 }.rank()
                < TemporalContainment::Open { distance: 100 }.rank()
        );
        assert!(
            TemporalContainment::Open { distance: 100 }.rank()
                < TemporalContainment::Outside.rank()
        );
    }

    #[test]
    fn containment_visibility_with_max_distance() {
        assert!(TemporalContainment::Atemporal.is_visible_within(0));
        assert!(TemporalContainment::Definite.is_visible_within(0));
        assert!(TemporalContainment::Open { distance: 10 }.is_visible_within(10));
        assert!(!TemporalContainment::Open { distance: 11 }.is_visible_within(10));
        assert!(!TemporalContainment::Outside.is_visible_within(i64::MAX));
    }
}
