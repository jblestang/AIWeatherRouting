use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinate {
    pub lat: f64,
    pub lon: f64,
}

impl Coordinate {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// Weather data for wind at a specific point
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindData {
    /// U component of wind (m/s)
    pub u: f32,
    /// V component of wind (m/s)
    pub v: f32,
}

impl WindData {
    pub fn speed(&self) -> f32 {
        (self.u.powi(2) + self.v.powi(2)).sqrt()
    }
    
    pub fn direction(&self) -> f32 {
        let angle = self.v.atan2(self.u).to_degrees();
        let mut dir = 270.0 - angle;
        if dir < 0.0 { dir += 360.0; }
        if dir >= 360.0 { dir -= 360.0; }
        dir
    }
}

use std::collections::HashMap;

/// Global resource to hold the loaded wind data for the frontend mapping
#[derive(Resource, Default, Debug, Clone)]
pub struct WindField {
    /// 1x1 degree spatial chunks storing data points. 
    /// Key: (lon.floor(), lat.floor())
    pub chunks: HashMap<(i32, i32), Vec<(Coordinate, WindData)>>,
}

impl WindField {
    pub fn insert_point(&mut self, coord: Coordinate, wind: WindData) {
        let chunk_x = coord.lon.floor() as i32;
        let chunk_y = coord.lat.floor() as i32;
        self.chunks.entry((chunk_x, chunk_y)).or_default().push((coord, wind));
    }
    
    pub fn get_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        if self.chunks.is_empty() {
            return None;
        }
        
        let mut min_lat = f64::MAX;
        let mut max_lat = f64::MIN;
        let mut min_lon = f64::MAX;
        let mut max_lon = f64::MIN;
        
        for block in self.chunks.values() {
            for (coord, _) in block {
                if coord.lat < min_lat { min_lat = coord.lat; }
                if coord.lat > max_lat { max_lat = coord.lat; }
                if coord.lon < min_lon { min_lon = coord.lon; }
                if coord.lon > max_lon { max_lon = coord.lon; }
            }
        }
        
        Some((min_lat, max_lat, min_lon, max_lon))
    }

    /// Finds the nearest wind data point to the given coordinate
    pub fn get_wind_at(&self, coord: &Coordinate) -> Option<WindData> {
        let chunk_x = coord.lon.floor() as i32;
        let chunk_y = coord.lat.floor() as i32;
        
        if let Some(chunk) = self.chunks.get(&(chunk_x, chunk_y)) {
            // Find nearest neighbor in this chunk
            let mut best_dist = f64::MAX;
            let mut best_wind = None;
            
            for (p_coord, wind) in chunk {
                let d_lat = p_coord.lat - coord.lat;
                let d_lon = p_coord.lon - coord.lon;
                let dist_sq = d_lat * d_lat + d_lon * d_lon;
                
                if dist_sq < best_dist {
                    best_dist = dist_sq;
                    best_wind = Some(*wind);
                }
            }
            return best_wind;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wind_direction_conventions() {
        // Navigational: 0=North (wind from North), 90=East, 180=South, 270=West
        
        // Wind from North: u=0, v=-1 (assuming V is North-South, U is East-West)
        // Wait, current implementation:
        // let angle = self.v.atan2(self.u).to_degrees();
        // let mut dir = 270.0 - angle;
        
        // Let's verify our assumptions:
        // angle = atan2(v, u)
        // North (0 deg): v < 0? No, usually v is positive North.
        // In GRIB: u > 0 is Eastward, v > 0 is Northward.
        // Wind FROM North: u=0, v=-5 => angle = atan2(-5, 0) = -90. dir = 270 - (-90) = 360 = 0. Correct.
        // Wind FROM East: u=-5, v=0 => angle = atan2(0, -5) = 180. dir = 270 - 180 = 90. Correct.
        // Wind FROM South: u=0, v=5 => angle = atan2(5, 0) = 90. dir = 270 - 90 = 180. Correct.
        // Wind FROM West: u=5, v=0 => angle = atan2(0, 5) = 0. dir = 270 - 0 = 270. Correct.
        
        let north_wind = WindData { u: 0.0, v: -5.0 };
        assert_eq!(north_wind.direction(), 0.0);
        
        let east_wind = WindData { u: -5.0, v: 0.0 };
        assert_eq!(east_wind.direction(), 90.0);
        
        let south_wind = WindData { u: 0.0, v: 5.0 };
        assert_eq!(south_wind.direction(), 180.0);
        
        let west_wind = WindData { u: 5.0, v: 0.0 };
        assert_eq!(west_wind.direction(), 270.0);
    }
}

/// Ocean current data
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurrentData {
    pub u: f32,
    pub v: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeaState {
    pub significant_wave_height: f32,
}

/// The state of the boat at a specific point in time
#[derive(Debug, Clone, PartialEq)]
pub struct BoatState {
    pub position: Coordinate,
    pub time: chrono::DateTime<chrono::Utc>,
    /// Elapsed time since departure in seconds
    pub elapsed_time: f64,
}
