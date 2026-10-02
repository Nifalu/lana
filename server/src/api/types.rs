//! Shared API payload types, wire formats, and validation rules.

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

/// A WGS84 point as carried by the API: longitude first, matching the GeoJSON
/// `[lon, lat]` order used everywhere else in the server.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LonLat {
    pub lon: f64,
    pub lat: f64,
}

impl LonLat {
    /// Both coordinates finite and within WGS84 bounds.
    pub fn validate(&self) -> Result<(), String> {
        validate_lon(self.lon)?;
        validate_lat(self.lat)
    }
}

/// Longitude within `[-180, 180]`.
pub fn validate_lon(lon: f64) -> Result<(), String> {
    if lon.is_finite() && (-180.0..=180.0).contains(&lon) {
        Ok(())
    } else {
        Err(format!("lon must be within [-180, 180], got {lon}"))
    }
}

/// Latitude within `[-90, 90]`.
pub fn validate_lat(lat: f64) -> Result<(), String> {
    if lat.is_finite() && (-90.0..=90.0).contains(&lat) {
        Ok(())
    } else {
        Err(format!("lat must be within [-90, 90], got {lat}"))
    }
}

/// Weekday 0=Monday..6=Sunday.
pub fn validate_weekday(weekday: i16) -> Result<(), String> {
    if (0..=6).contains(&weekday) {
        Ok(())
    } else {
        Err(format!(
            "weekday must be 0 (Monday)..6 (Sunday), got {weekday}"
        ))
    }
}

/// Radius in meters, strictly positive and finite.
pub fn validate_radius_m(radius_m: f64) -> Result<(), String> {
    if radius_m.is_finite() && radius_m > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "radius_m must be a finite number greater than 0, got {radius_m}"
        ))
    }
}

/// The window's end must be strictly after its start (no overnight windows).
pub fn validate_time_range(start_time: NaiveTime, end_time: NaiveTime) -> Result<(), String> {
    if end_time > start_time {
        Ok(())
    } else {
        Err(format!(
            "end_time ({end_time}) must be after start_time ({start_time})"
        ))
    }
}

/// Parses a Europe/Zurich local wall time from "HH:MM" or "HH:MM:SS".
///
/// Times are wall times by convention (ADR 0004): the server stores
/// time-of-day only; applying the Europe/Zurich timezone is matching's job
/// (ticket 05).
pub fn parse_wall_time(s: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(s, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .map_err(|_| format!("invalid time '{s}': expected HH:MM or HH:MM:SS"))
}

/// Serde wire format for wall times: "HH:MM" or "HH:MM:SS" in, always
/// "HH:MM:SS" out. Attach with `#[serde(with = "wall_time")]`.
pub mod wall_time {
    use super::*;

    pub fn serialize<S>(time: &NaiveTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&time.format("%H:%M:%S"))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_wall_time(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(h: u32, m: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, s).unwrap()
    }

    #[test]
    fn weekday_bounds() {
        for weekday in 0..=6 {
            assert_eq!(validate_weekday(weekday), Ok(()));
        }
        assert!(validate_weekday(-1).is_err());
        assert!(validate_weekday(7).is_err());
    }

    #[test]
    fn radius_must_be_positive_and_finite() {
        assert_eq!(validate_radius_m(500.0), Ok(()));
        assert_eq!(validate_radius_m(0.001), Ok(()));
        assert!(validate_radius_m(0.0).is_err());
        assert!(validate_radius_m(-5.0).is_err());
        assert!(validate_radius_m(f64::NAN).is_err());
        assert!(validate_radius_m(f64::INFINITY).is_err());
    }

    #[test]
    fn lon_lat_bounds() {
        assert!(validate_lon(-180.0).is_ok());
        assert!(validate_lon(180.0).is_ok());
        assert!(validate_lon(180.5).is_err());
        assert!(validate_lon(f64::NAN).is_err());
        assert!(validate_lat(-90.0).is_ok());
        assert!(validate_lat(90.0).is_ok());
        assert!(validate_lat(90.5).is_err());
    }

    #[test]
    fn end_time_must_be_strictly_after_start_time() {
        assert_eq!(validate_time_range(time(9, 0, 0), time(17, 0, 0)), Ok(()));
        assert!(validate_time_range(time(9, 0, 0), time(9, 0, 0)).is_err());
        assert!(validate_time_range(time(17, 0, 0), time(9, 0, 0)).is_err());
    }

    #[test]
    fn wall_time_parsing() {
        assert_eq!(parse_wall_time("09:00"), Ok(time(9, 0, 0)));
        assert_eq!(parse_wall_time("09:00:00"), Ok(time(9, 0, 0)));
        assert_eq!(parse_wall_time("23:59:59"), Ok(time(23, 59, 59)));
        assert!(parse_wall_time("25:00").is_err());
        assert!(parse_wall_time("9am").is_err());
        assert!(parse_wall_time("").is_err());
    }

    /// The serde module round-trips "09:00" through the canonical
    /// "HH:MM:SS" wire format.
    #[test]
    fn wall_time_serde_round_trip() {
        #[derive(Serialize, Deserialize)]
        struct Doc {
            #[serde(with = "wall_time")]
            at: NaiveTime,
        }
        let doc: Doc = serde_json::from_str(r#"{ "at": "09:30" }"#).unwrap();
        assert_eq!(doc.at, time(9, 30, 0));
        assert_eq!(
            serde_json::to_value(&doc).unwrap(),
            serde_json::json!({ "at": "09:30:00" })
        );
    }
}
