use AIWeatherRouting::engine::router::IsochroneRouter;
use AIWeatherRouting::engine::models::{Coordinate, WindData, CurrentData, BoatState};
use AIWeatherRouting::engine::physics::PhysicsModel;
use AIWeatherRouting::parsers::polars::PolarData;
use AIWeatherRouting::engine::mask::LandMask;
use std::time::Instant;

fn main() {
    env_logger::init();
    println!("--- AI Weather Routing CLI Debugger ---");

    // 1. Setup Data
    let start = Coordinate::new(48.0, -5.0); // Off the coast of Brittany
    let destination = Coordinate::new(40.0, -10.0); // Towards Azores
    let time_step = 3600.0; // 1 hour

    println!("Loading Polar...");
    let polar = PolarData::load_from_csv("data/imoca_60.csv");
    println!("Polar loaded: {} TWA, {} TWS points", polar.twa.len(), polar.tws.len());

    let physics = PhysicsModel::new();
    let land_mask = LandMask::new(); // Empty mask for simplicity

    let mut router = IsochroneRouter::new(start, destination, time_step);
    
    let initial_state = BoatState {
        position: start,
        time: chrono::Utc::now(),
        elapsed_time: 0.0,
    };

    let mut current_front = vec![initial_state];

    // 2. Run Steps
    for step in 1..=5 {
        let start_time = Instant::now();
        println!("\n--- Step {} ---", step);
        
        // Simple uniform wind: 20 knots from North (TWD 0)
        // 20 knots = 10.288 m/s
        // Wind FROM North => u=0, v=-10.288
        let wind_data = WindData { u: 0.0, v: -10.288 };

        current_front = router.step(
            &current_front,
            &physics,
            &polar,
            &land_mask,
            |_| wind_data,
            |_| CurrentData { u: 0.0, v: 0.0 }
        );

        let duration = start_time.elapsed();
        println!("Front points: {}", current_front.len());
        println!("Calculation time: {:?}", duration);

        if let Some(first) = current_front.first() {
            println!("First point: Lat: {:.4}, Lon: {:.4}, Elapsed: {}h", 
                first.position.lat, first.position.lon, first.elapsed_time / 3600.0);
        }
    }

    println!("\nDebug completed.");
}
