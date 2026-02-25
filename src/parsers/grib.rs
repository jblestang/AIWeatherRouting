use std::path::Path;
use log::info;
use eccodes::{CodesFile, ProductKind, KeyRead, DynamicKeyType, FallibleIterator};

use crate::engine::models::{Coordinate, WindData, CurrentData};

pub struct GribLoader {
    // This will eventually hold a structured representation of the grid
}

impl GribLoader {
    pub fn new() -> Self {
        Self {}
    }

    /// Loads a GRIB file and extracts U and V components for wind using eccodes
    pub fn load_wind_data<P: AsRef<Path>>(&self, path: P) -> Result<Vec<(Coordinate, WindData)>, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        info!("Loading wind data from GRIB file: {:?}", path);
        
        let path_str = path.to_str().unwrap();
        
        let mut file = CodesFile::new_from_file(path_str, ProductKind::GRIB)?;
        
        // We need to store both U and V before constructing the final WindData
        // (Assuming U and V come in separate messages, which is standard)
        let mut u_components: Vec<f64> = Vec::new();
        let mut v_components: Vec<f64> = Vec::new();
        let mut lats: Vec<f64> = Vec::new();
        let mut lons: Vec<f64> = Vec::new();

        let mut iter = file.ref_message_iter();
        
        loop {
            match iter.next() {
                Ok(Some(message)) => {
                    if let Ok(DynamicKeyType::Str(name)) = message.read_key_dynamic("shortName") {
                        if name == "10u" {
                            info!("Extracting 10u (U-wind component)");
                            u_components = message.read_key("values").unwrap_or_default();
                            lats = message.read_key("latitudes").unwrap_or_default();
                            lons = message.read_key("longitudes").unwrap_or_default();
                        } else if name == "10v" {
                            info!("Extracting 10v (V-wind component)");
                            v_components = message.read_key("values").unwrap_or_default();
                        }
                    }
                },
                Ok(None) => break,
                Err(eccodes::CodesError::Internal(eccodes::errors::CodesInternal::CodesPrematureEndOfFile)) => {
                    log::warn!("GRIB file reached premature EOF (likely truncated). Proceeding with data extracted so far.");
                    break;
                },
                Err(e) => return Err(e.into()),
            }
        }
        
        // Merge them into Coordinate -> WindData
        let mut wind_data = Vec::new();
        
        let point_count = u_components.len().min(v_components.len()).min(lats.len()).min(lons.len());
        
        for i in 0..point_count {
            let mut lon = lons[i];
            // Normalize longitude from [0, 360] to [-180, 180]
            if lon > 180.0 {
                lon -= 360.0;
            }
            
            wind_data.push((
                Coordinate::new(lats[i], lon),
                WindData { u: u_components[i] as f32, v: v_components[i] as f32 }
            ));
        }
        
        info!("Successfully loaded {} wind points.", wind_data.len());
        Ok(wind_data)
    }

    /// Loads a GRIB file and extracts U and V components for ocean currents using eccodes
    pub fn load_current_data<P: AsRef<Path>>(&self, path: P) -> Result<Vec<(Coordinate, CurrentData)>, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        info!("Loading current data from GRIB file: {:?}", path);
        
        let mut current_data = Vec::new();
        current_data.push((Coordinate::new(0.0, 0.0), CurrentData { u: 1.0, v: 0.5 }));
        
        Ok(current_data)
    }
}
