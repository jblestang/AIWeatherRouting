use roaring::RoaringTreemap;
use bevy::prelude::Resource;
use crate::engine::models::Coordinate;
use log::info;

pub const NX: u64 = 86400;
pub const NY: u64 = 43200;

#[derive(Resource)]
pub struct LandMask {
    pub mask: RoaringTreemap,
}

impl LandMask {
    pub fn new() -> Self {
        Self {
            mask: RoaringTreemap::new(),
        }
    }

    /// Loads a high-resolution GSHHG mask from a compressed file
    pub fn load() -> Self {
        use std::io::BufReader;
        use xz2::read::XzDecoder;
        
        info!("Loading high-resolution GSHHG land mask from assets/gshhg_mask.tbmap.xz");
        
        let path = "assets/gshhg_mask.tbmap.xz";
        let file = std::fs::File::open(path).expect("Failed to open land mask file. Did you copy it to assets/gshhg_mask.tbmap.xz?");
        let reader = BufReader::new(file);
        let decoder = XzDecoder::new(reader);
        
        let mask = RoaringTreemap::deserialize_from(decoder)
            .expect("Failed to deserialize land mask treemap");
            
        info!("Land mask loaded successfully.");
        
        Self { mask }
    }

    fn coords_to_indices(&self, lon: f64, lat: f64) -> (u64, u64) {
        // Affine transform: sa = 240, sc = 43200, se = 240, sf = 21600
        let x = (lon * 240.0 + 43200.0) as u64;
        let y = (lat * 240.0 + 21600.0) as u64;
        (x.clamp(0, NX - 1), y.clamp(0, NY - 1))
    }

    /// Adds a rectangular bounding box of coordinates to the roaring bitmap as Land (for tests)
    pub fn add_land_box(&mut self, min_lon: f64, max_lon: f64, min_lat: f64, max_lat: f64) {
        let (min_x, min_y) = self.coords_to_indices(min_lon, min_lat);
        let (max_x, max_y) = self.coords_to_indices(max_lon, max_lat);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.mask.insert(y * NX + x);
            }
        }
    }

    /// Checks if a coordinate is over land
    pub fn is_land(&self, coord: &Coordinate) -> bool {
        let (x, y) = self.coords_to_indices(coord.lon, coord.lat);
        if y >= NY { return false; }
        self.mask.contains(y * NX + x)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_land_mask_classification() {
        let mask = LandMask::load();
        
        // Paris (Land)
        let paris = Coordinate::new(48.8566, 2.3522);
        assert!(mask.is_land(&paris), "Paris should be on land");
        
        // Mid-Atlantic (Sea)
        let sea = Coordinate::new(40.0, -30.0);
        assert!(!mask.is_land(&sea), "Mid-Atlantic should be at sea");
    }
}
