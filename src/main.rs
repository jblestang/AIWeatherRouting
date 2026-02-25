use bevy::prelude::*;
use log::info;
use AIWeatherRouting::ui;

fn main() {
    // Initialize env logger for basic logging
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
        
    info!("Starting AIWeatherRouting application...");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ui::UiPlugin)
        .add_systems(Startup, init_routing_engine)
        .run();
}

fn init_routing_engine() {
    // This system will be responsible for instantiating the IsochroneRouter,
    // loading the OpenSeaMap mask, Polars, and initial GRIB data.
    info!("Initializing routing engine and loading base data...");
}
