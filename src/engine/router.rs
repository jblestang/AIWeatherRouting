use crate::engine::models::{BoatState, Coordinate, WindData, CurrentData};
use crate::engine::physics::PhysicsModel;
use crate::engine::mask::LandMask;
use bevy::prelude::*;
use rayon::prelude::*;
use log::info;

#[derive(Resource)]
pub struct RoutingState {
    pub is_playing: bool,
    pub step_timer: Timer,
    pub router: IsochroneRouter,
    pub fronts: Vec<Vec<BoatState>>,
}

impl Default for RoutingState {
    fn default() -> Self {
        // Saint Malo
        let start = Coordinate::new(48.66, -2.03);
        // Saint Florent, Corsica
        let destination = Coordinate::new(42.68, 9.30);
        
        // 1 hour time step
        let time_step = 3600.0;
        
        let initial_state = BoatState {
            position: start,
            time: chrono::Utc::now(),
            elapsed_time: 0.0,
        };

        Self {
            is_playing: false,
            step_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            router: IsochroneRouter::new(start, destination, time_step),
            fronts: vec![vec![initial_state]],
        }
    }
}

pub struct IsochroneRouter {
    pub start: Coordinate,
    pub destination: Coordinate,
    /// Time step in seconds
    pub time_step: f64, 
    pub grid_precision: f64,
}

impl IsochroneRouter {
    pub fn new(start: Coordinate, destination: Coordinate, time_step: f64) -> Self {
        Self { start, destination, time_step, grid_precision: 400.0 }
    }

    /// Helper to calculate the bearing between two coordinates
    pub fn calculate_bearing(start: &Coordinate, end: &Coordinate) -> f32 {
        let start_lat = start.lat.to_radians();
        let start_lon = start.lon.to_radians();
        let end_lat = end.lat.to_radians();
        let end_lon = end.lon.to_radians();

        let d_lon = end_lon - start_lon;

        let y = d_lon.sin() * end_lat.cos();
        let x = start_lat.cos() * end_lat.sin() - start_lat.sin() * end_lat.cos() * d_lon.cos();
        y.atan2(x).to_degrees() as f32
    }

    /// Helper to calculate the great-circle distance between two coordinates in meters
    pub fn calculate_distance(start: &Coordinate, end: &Coordinate) -> f64 {
        let r_earth = 6_371_000.0;
        let start_lat = start.lat.to_radians();
        let end_lat = end.lat.to_radians();
        let d_lat = (end.lat - start.lat).to_radians();
        let d_lon = (end.lon - start.lon).to_radians();

        let a = (d_lat / 2.0).sin().powi(2) + 
                start_lat.cos() * end_lat.cos() * (d_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        r_earth * c
    }

    /// Computes the new coordinate given a starting point, heading (COG) and distance (meters)
    pub fn calculate_destination(start: &Coordinate, distance_m: f64, bearing_deg: f32) -> Coordinate {
        let r_earth = 6_371_000.0; // Radius of Earth in meters
        let angular_dist = distance_m / r_earth;
        let bearing_rad = (bearing_deg as f64).to_radians();

        let start_lat = start.lat.to_radians();
        let start_lon = start.lon.to_radians();

        let end_lat = (start_lat.sin() * angular_dist.cos() + 
                       start_lat.cos() * angular_dist.sin() * bearing_rad.cos()).asin();

        let end_lon = start_lon + (bearing_rad.sin() * angular_dist.sin() * start_lat.cos())
            .atan2(angular_dist.cos() - start_lat.sin() * end_lat.sin());

        Coordinate {
            lat: end_lat.to_degrees(),
            lon: end_lon.to_degrees(),
        }
    }

    /// Performs one step of the isochrone expansion
    pub fn step(
        &mut self, 
        current_front: &[BoatState], 
        physics: &PhysicsModel,
        polar: &crate::parsers::polars::PolarData,
        land_mask: &LandMask,
        wind_at: impl Fn(&Coordinate) -> WindData + Sync, // Placeholder for data grid lookup
        current_at: impl Fn(&Coordinate) -> CurrentData + Sync, // Placeholder for data grid lookup
    ) -> Vec<BoatState> {
        info!("Expanding isochrone front for {} points", current_front.len());
        
        // Define the search fan (widened to allow for tacking/wearing)
        let num_headings = 360;
        let max_angle = 180.0; // Sweep from -180 to +180 degrees
        let angle_step = (max_angle * 2.0) / (num_headings as f32 - 1.0);

        let next_front_candidates: Vec<BoatState> = current_front.par_iter().flat_map(|state| {
            let direct_bearing = Self::calculate_bearing(&state.position, &self.destination);
            let mut local_candidates = Vec::with_capacity(num_headings);

            for i in 0..num_headings {
                let offset = -max_angle + (i as f32 * angle_step);
                let mut test_heading = direct_bearing + offset;
                
                // Normalize heading to [0, 360)
                if test_heading < 0.0 { test_heading += 360.0; }
                if test_heading >= 360.0 { test_heading -= 360.0; }

                // 2. Fetch environmental data at current position
                let wind = wind_at(&state.position);
                let current = current_at(&state.position);

                // 3. Compute the SOG/COG using PhysicsModel
                let (sog, cog) = physics.compute_vector(test_heading, &wind, &current, polar, None);

                if sog <= 0.001 {
                    // Only warn once per step
                    static mut LAST_LOGGED_STEP: f64 = -1.0;
                    
                    unsafe {
                        if LAST_LOGGED_STEP != state.elapsed_time {
                            if polar.tws.is_empty() {
                                log::error!("Router STALLED: No Polar Data loaded. Expansion impossible.");
                            } else {
                                log::warn!("Zero SOG detected at {:?}! Is wind data missing or boat pinched? (Wind: {:?})", state.position, wind);
                            }
                            LAST_LOGGED_STEP = state.elapsed_time;
                        }
                    }
                }

                // Calculate distance traveled in this time step: Distance = Speed * Time
                let distance_m = (sog as f64) * self.time_step;

                // 4. Calculate new geographical position
                let new_position = Self::calculate_destination(&state.position, distance_m, cog);

                // 5. Check Land Collision via `LandMask`
                if !land_mask.is_land(&new_position) {
                    local_candidates.push(BoatState {
                        position: new_position,
                        time: state.time + chrono::Duration::seconds(self.time_step as i64),
                        elapsed_time: state.elapsed_time + self.time_step,
                    });
                }
            }
            local_candidates
        }).collect();
        
        // --- Pass 1: Density Pruning (Grid Bucketing) ---
        let mut grid: std::collections::HashMap<(i32, i32), BoatState> = std::collections::HashMap::new();
        let precision = self.grid_precision;
        
        for state in next_front_candidates {
            let grid_x = (state.position.lon * precision).round() as i32;
            let grid_y = (state.position.lat * precision).round() as i32;
            
            let dist_to_dest = Self::calculate_distance(&state.position, &self.destination);
            
            grid.entry((grid_x, grid_y))
                .and_modify(|existing| {
                    if dist_to_dest < Self::calculate_distance(&existing.position, &self.destination) {
                        *existing = state.clone();
                    }
                })
                .or_insert(state);
        }

        // --- Pass 2: Boundary Detection (Keep only the "Frontier") ---
        // A point is on the frontier if it has at least one empty neighbor in the grid
        let next_front: Vec<BoatState> = grid.iter()
            .filter(|((x, y), _)| {
                // Check 4-connected neighbors
                let neighbors = [(x + 1, *y), (x - 1, *y), (*x, y + 1), (*x, y - 1)];
                neighbors.iter().any(|neighbor| !grid.contains_key(neighbor))
            })
            .map(|(_, state)| state.clone())
            .collect();

        info!("Pruned front down to {} optimal frontier points", next_front.len());
        
        next_front
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::models::WindData;
    use crate::parsers::polars::PolarData;

    #[test]
    fn test_isochrone_router_default_step() {
        let state = RoutingState::default();
        assert_eq!(state.router.time_step, 3600.0, "Default time step should be 1 hour (3600 seconds)");
    }

    #[test]
    fn test_router_expansion() {
        let start = Coordinate::new(45.0, -1.0);
        let dest = Coordinate::new(46.0, -1.0); // North
        let mut router = IsochroneRouter::new(start, dest, 3600.0);
        
        let physics = PhysicsModel::new();
        let land_mask = LandMask::new(); // Empty mask
        
        // Setup simple polar: always 5 knots
        let mut polar = PolarData::default();
        polar.tws = vec![0.0, 10.0, 20.0];
        polar.twa = vec![0.0, 90.0, 180.0];
        polar.speeds = vec![
            vec![5.0, 5.0, 5.0],
            vec![5.0, 5.0, 5.0],
            vec![5.0, 5.0, 5.0],
        ];

        let initial_state = BoatState {
            position: start,
            time: chrono::Utc::now(),
            elapsed_time: 0.0,
        };

        let next_front = router.step(
            &[initial_state],
            &physics,
            &polar,
            &land_mask,
            |_| WindData { u: 0.0, v: 5.0 }, // 5m/s North wind (dir 180 - South)
            |_| CurrentData { u: 0.0, v: 0.0 }
        );

        assert!(next_front.len() > 1, "Router should expand to multiple points, got {}", next_front.len());
        
        // Verify points moved
        for state in next_front {
            assert!(state.position.lat > 45.0 || state.position.lat < 45.0 || state.position.lon != -1.0);
        }
    }

    #[test]
    fn test_router_land_avoidance() {
        // Create a land barrier between start and destination using REAL land data
        // Start: South of Isle of Wight, Dest: North of Isle of Wight
        let start = Coordinate::new(50.5, -1.35); // South of Needles
        let dest = Coordinate::new(50.8, -1.35);  // North of Isle of Wight
        let mut router = IsochroneRouter::new(start, dest, 1800.0); // 30 min steps
        
        let physics = PhysicsModel::new();
        let mut polar = PolarData::default();
        polar.tws = vec![0.0, 20.0];
        polar.twa = vec![0.0, 180.0];
        polar.speeds = vec![vec![10.0, 10.0], vec![10.0, 10.0]];

        let land_mask = LandMask::load();
        let initial_state = BoatState {
            position: start,
            time: chrono::Utc::now(),
            elapsed_time: 0.0,
        };

        let next_front = router.step(
            &[initial_state],
            &physics,
            &polar,
            &land_mask,
            |_| WindData { u: 0.0, v: 15.0 }, // Strong South wind, moving North
            |_| CurrentData { u: 0.0, v: 0.0 }
        );

        for state in &next_front {
            assert!(!land_mask.is_land(&state.position), "Point should not be on land: {:?}", state.position);
        }
    }

    #[test]
    fn test_router_zero_speed() {
        let start = Coordinate::new(45.0, -1.0);
        let dest = Coordinate::new(46.0, -1.0);
        let mut router = IsochroneRouter::new(start, dest, 3600.0);
        
        let physics = PhysicsModel::new();
        let land_mask = LandMask::new();
        
        let polar = PolarData::default(); // Empty = 0 speed

        let initial_state = BoatState {
            position: start,
            time: chrono::Utc::now(),
            elapsed_time: 0.0,
        };

        let next_front = router.step(
            &[initial_state],
            &physics,
            &polar,
            &land_mask,
            |_| WindData { u: 10.0, v: 10.0 }, 
            |_| CurrentData { u: 0.0, v: 0.0 }
        );

        // Should collapse to 1 point (the start point)
        assert_eq!(next_front.len(), 1);
        assert!((next_front[0].position.lat - start.lat).abs() < 1e-10);
    }

    #[test]
    fn test_router_with_actual_polar() {
        let start = Coordinate::new(45.0, -1.0);
        let dest = Coordinate::new(47.0, -1.0);
        let mut router = IsochroneRouter::new(start, dest, 3600.0);
        
        let physics = PhysicsModel::new();
        let land_mask = LandMask::new();
        
        // Load real polar data
        let polar = PolarData::load_from_csv("data/imoca_60.csv");

        let initial_state = BoatState {
            position: start,
            time: chrono::Utc::now(),
            elapsed_time: 0.0,
        };

        let next_front = router.step(
            &[initial_state],
            &physics,
            &polar,
            &land_mask,
            |_| WindData { u: 0.0, v: 10.0 }, // 10m/s North wind
            |_| CurrentData { u: 0.0, v: 0.0 }
        );

        assert!(next_front.len() > 10, "Should expand significantly with IMOCA polar, got {}", next_front.len());
    }
}
