//! Shared API payload types, wire formats, and validation rules.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
