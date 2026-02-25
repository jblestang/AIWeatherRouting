// Placeholder for Sail Boat Polars loader

use std::path::Path;
use std::fs::File;
use std::io::{BufRead, BufReader};
use log::info;
use bevy::prelude::Resource;

#[derive(Debug, Clone, Resource, Default)]
pub struct PolarData {
    /// True Wind Speeds (knots)
    pub tws: Vec<f32>,
    /// True Wind Angles (degrees)
    pub twa: Vec<f32>,
    /// Boat speeds in knots: speeds[twa_idx][tws_idx]
    pub speeds: Vec<Vec<f32>>,
}

impl PolarData {
    pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Self {
        info!("Loading polar data from CSV: {:?}", path.as_ref());
        
        let file = File::open(path).expect("Unable to open polar CSV");
        let reader = BufReader::new(file);
        
        let mut lines = reader.lines();
        
        // First line is header: "twa/tws", "5", "10", ...
        let header = lines.next().expect("Empty CSV").expect("Format error");
        let parts: Vec<&str> = header.split(',').collect();
        let tws_str: Vec<&str> = parts[1..].to_vec();
        
        let mut tws: Vec<f32> = Vec::new();
        for val in tws_str {
            tws.push(val.parse().unwrap_or(0.0));
        }
        
        let mut twa_list = Vec::new();
        let mut speeds = Vec::new();

        for line_result in lines {
            let line = line_result.expect("Error reading row");
            if line.trim().is_empty() {
                continue;
            }
            let row_parts: Vec<&str> = line.split(',').collect();
            
            let twa_val: f32 = row_parts[0].parse().unwrap_or(0.0);
            twa_list.push(twa_val);
            
            let mut row_speeds = Vec::new();
            for val in &row_parts[1..] {
                row_speeds.push(val.parse().unwrap_or(0.0));
            }
            speeds.push(row_speeds);
        }

        Self {
            tws,
            twa: twa_list,
            speeds,
        }
    }

    /// Bilinear interpolation to find the boat speed (in knots) for a given TWS and TWA.
    pub fn get_speed(&self, target_tws: f32, target_twa: f32) -> f32 {
        if self.tws.is_empty() || self.twa.is_empty() {
            return 0.0;
        }

        // 1. Clamp bounds
        let tws_clamped = target_tws.clamp(self.tws[0], *self.tws.last().unwrap());
        let twa_clamped = target_twa.clamp(self.twa[0], *self.twa.last().unwrap());

        // 2. Find bounding indices for TWS
        let mut tws_idx0 = 0;
        let mut tws_idx1 = self.tws.len() - 1;
        
        for i in 0..self.tws.len() - 1 {
            if tws_clamped >= self.tws[i] && tws_clamped <= self.tws[i+1] {
                tws_idx0 = i;
                tws_idx1 = i + 1;
                break;
            }
        }

        // 3. Find bounding indices for TWA
        let mut twa_idx0 = 0;
        let mut twa_idx1 = self.twa.len() - 1;
        
        for i in 0..self.twa.len() - 1 {
            if twa_clamped >= self.twa[i] && twa_clamped <= self.twa[i+1] {
                twa_idx0 = i;
                twa_idx1 = i + 1;
                break;
            }
        }

        // 4. Extract 4 points
        let tws0 = self.tws[tws_idx0];
        let tws1 = self.tws[tws_idx1];
        let twa0 = self.twa[twa_idx0];
        let twa1 = self.twa[twa_idx1];

        let val00 = self.speeds[twa_idx0][tws_idx0];
        let val01 = self.speeds[twa_idx0][tws_idx1];
        let val10 = self.speeds[twa_idx1][tws_idx0];
        let val11 = self.speeds[twa_idx1][tws_idx1];

        // 5. Bilinear Interpolation
        if tws_idx0 == tws_idx1 && twa_idx0 == twa_idx1 {
            return val00;
        }

        let tws_frac = if tws0 == tws1 { 0.0 } else { (tws_clamped - tws0) / (tws1 - tws0) };
        let twa_frac = if twa0 == twa1 { 0.0 } else { (twa_clamped - twa0) / (twa1 - twa0) };

        let val0 = val00 * (1.0 - tws_frac) + val01 * tws_frac; // Interpolate across TWS at TWA 0
        let val1 = val10 * (1.0 - tws_frac) + val11 * tws_frac; // Interpolate across TWS at TWA 1

        val0 * (1.0 - twa_frac) + val1 * twa_frac // Interpolate across TWA
    }
}
