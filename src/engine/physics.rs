use crate::engine::models::{WindData, CurrentData, SeaState};
use crate::parsers::polars::PolarData;

pub struct PhysicsModel;

impl PhysicsModel {
    pub fn new() -> Self {
        Self
    }

    /// Calculates True Wind Angle (TWA) from True Wind Direction (TWD) and Boat Heading
    pub fn calculate_twa(twd: f32, heading: f32) -> f32 {
        let mut twa = twd - heading;
        while twa > 180.0 { twa -= 360.0; }
        while twa < -180.0 { twa += 360.0; }
        twa.abs() // Usually polars are symmetric
    }

    /// Computes the boat speed over Ground (SOG) and Course over Ground (COG)
    pub fn compute_vector(
        &self,
        heading: f32, // true heading (0=North, 90=East)
        wind: &WindData,
        current: &CurrentData,
        polar: &PolarData,
        _sea_state: Option<&SeaState>
    ) -> (f32, f32) { // returns (SOG in m/s, COG in degrees)
        // 1. Calculate TWS and TWD from WindData components
        let tws_ms = wind.speed();
        let twd = wind.direction();

        // 2. Calculate TWA
        let twa = Self::calculate_twa(twd, heading);

        // 3. Lookup Boat Speed through water (STW) from polars
        // Convert TWS to knots for polar lookup
        let tws_kts = tws_ms * 1.94384;
        let stw_kts = polar.get_speed(tws_kts, twa);
        let stw = stw_kts / 1.94384; // back to m/s

        // 4. Calculate boat velocity vector (East, North) relative to water
        let heading_rad = (heading as f64).to_radians();
        let boat_vx = stw as f64 * heading_rad.sin(); // East component
        let boat_vy = stw as f64 * heading_rad.cos(); // North component

        // 5. Add ocean current vector
        let sog_x = boat_vx + current.u as f64;
        let sog_y = boat_vy + current.v as f64;

        // 6. Calculate SOG and COG
        let sog = (sog_x.powi(2) + sog_y.powi(2)).sqrt() as f32;
        let mut cog = sog_x.atan2(sog_y).to_degrees() as f32;
        if cog < 0.0 { cog += 360.0; }

        (sog, cog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::models::Coordinate;

    #[test]
    fn test_calculate_twa() {
        // TWD 0 (North), Heading 0 (North) -> TWA 0
        assert_eq!(PhysicsModel::calculate_twa(0.0, 0.0), 0.0);
        // TWD 0, Heading 90 (East) -> TWA 90
        assert_eq!(PhysicsModel::calculate_twa(0.0, 90.0), 90.0);
        // TWD 0, Heading 180 (South) -> TWA 180
        assert_eq!(PhysicsModel::calculate_twa(0.0, 180.0), 180.0);
        // TWD 0, Heading 270 (West) -> TWA 90 (symmetric)
        assert_eq!(PhysicsModel::calculate_twa(0.0, 270.0), 90.0);
        
        // TWD 180 (South), Heading 0 -> TWA 180
        assert_eq!(PhysicsModel::calculate_twa(180.0, 0.0), 180.0);
        // TWD 180, Heading 150 -> TWA 30
        assert_eq!(PhysicsModel::calculate_twa(180.0, 150.0), 30.0);
    }

    #[test]
    fn test_compute_vector_no_current() {
        let physics = PhysicsModel::new();
        let mut polar = PolarData::default();
        polar.tws = vec![0.0, 10.0];
        polar.twa = vec![0.0, 180.0];
        polar.speeds = vec![vec![0.0, 10.0], vec![0.0, 10.0]]; // 0 at 0 TWS, 10 at 10 TWS

        let wind = WindData { u: 0.0, v: -5.144 }; // From North (TWD 0), 10 knots
        let current = CurrentData { u: 0.0, v: 0.0 };

        // Heading East (90)
        let (sog, cog) = physics.compute_vector(90.0, &wind, &current, &polar, None);
        
        // stw = 10 knots = 5.144 m/s
        assert!((sog - 5.144).abs() < 0.01);
        assert!((cog - 90.0).abs() < 0.1);

        // Heading North (0)
        let (sog, cog) = physics.compute_vector(0.0, &wind, &current, &polar, None);
        assert!((sog - 5.144).abs() < 0.01);
        assert!((cog - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_compute_vector_with_current() {
        let physics = PhysicsModel::new();
        let mut polar = PolarData::default();
        polar.tws = vec![0.0, 10.0];
        polar.twa = vec![0.0, 180.0];
        polar.speeds = vec![vec![0.0, 10.0], vec![0.0, 10.0]]; // 0 at 0 TWS, 10 at 10

        let wind = WindData { u: 0.0, v: 0.0 }; // No wind
        let current = CurrentData { u: 2.0, v: 0.0 }; // 2 m/s East current

        // Heading North (0)
        let (sog, cog) = physics.compute_vector(0.0, &wind, &current, &polar, None);
        
        // stw = 0 (no wind), so we just drift with current
        assert!((sog - 2.0).abs() < 0.1);
        assert!((cog - 90.0).abs() < 0.1); // Course should be East
    }
}
