use bevy::{
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
    tasks::{IoTaskPool, Task},
    window::PrimaryWindow,
    render::camera::CameraProjection,
};
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use crate::engine::models::WindField;
use crate::engine::mask::LandMask;
use crate::engine::router::RoutingState;
use crate::engine::physics::PhysicsModel;
use crate::parsers::grib::GribLoader;
use crate::parsers::polars::PolarData;

pub mod map;
use map::{render_openseamap_system, render_wind_barbules_system, TileManager};

pub struct UiPlugin;

#[derive(Component)]
pub struct AsyncGribLoadTask(Task<Vec<(crate::engine::models::Coordinate, crate::engine::models::WindData)>>);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let mask = LandMask::load();
        
        app.add_plugins(EguiPlugin)
            .init_resource::<TileManager>()
            .init_resource::<WindField>()
            .init_resource::<PolarData>()
            .init_resource::<RoutingState>()
            .insert_resource(mask)
            .add_systems(Startup, (setup_camera, startup_load_grib))
            .add_systems(
                Update,
                (
                    handle_grib_load_task,
                    routing_step_system,
                    ui_panel_system,
                    render_openseamap_system,
                    map::render_grid_system,
                    map::render_isochrones_system,
                    render_wind_barbules_system,
                ),
            )
            .add_systems(Update, camera_movement_system);
    }
}

fn setup_camera(mut commands: Commands) {
    let camera = Camera2d;
    // project_mercator for France (Lat: 46.2, Lon: 2.2) at zoom 1
    // let's do it manually or via project_mercator if available in mod.rs. 
    // It's in map::project_mercator. 
    use crate::engine::models::Coordinate;
    let france_coord = Coordinate { lat: 46.2, lon: 2.2 };
    let center = map::project_mercator(&france_coord, 1);
    
    commands.spawn((
        camera,
        Transform::from_xyz(center.x, center.y, 0.0)
            .with_scale(Vec3::new(0.05, 0.05, 1.0)), // Zoom in
    ));
}

fn ui_panel_system(
    mut contexts: EguiContexts, 
    wind_field: Res<WindField>,
    mut polar_data: ResMut<PolarData>,
    mut routing_state: ResMut<RoutingState>,
    land_mask: Res<LandMask>,
) {
    egui::Window::new("AI Weather Routing Debugger")
        .default_size([400.0, 500.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.heading("Controls");
            
            if ui.button("Load Current GRIB").clicked() {
                // Implementation for loading current data
            }
            if ui.button("Load IMOCA 60 Polar").clicked() {
                *polar_data = PolarData::load_from_csv("data/imoca_60.csv");
            }
            
            ui.separator();
            ui.heading("GRIB Info");
            if let Some((min_lat, max_lat, min_lon, max_lon)) = wind_field.get_bounds() {
                ui.label(format!("Latitude range: {:.2}° to {:.2}°", min_lat, max_lat));
                ui.label(format!("Longitude range: {:.2}° to {:.2}°", min_lon, max_lon));
                let points_count: usize = wind_field.chunks.values().map(|v| v.len()).sum();
                ui.label(format!("Total points: {}", points_count));
            } else {
                ui.label("Waiting for background load...");
            }
            
            ui.separator();
            ui.heading("Polar Viewer");
            if !polar_data.twa.is_empty() {
                egui::ScrollArea::both().max_height(200.0).show(ui, |ui| {
                    egui::Grid::new("polar_grid").striped(true).show(ui, |ui| {
                        ui.label("TWA \\ TWS");
                        for tws in &polar_data.tws {
                            ui.label(format!("{} kt", tws));
                        }
                        ui.end_row();
                        
                        for (i, twa) in polar_data.twa.iter().enumerate() {
                            ui.label(format!("{}°", twa));
                            for speed in &polar_data.speeds[i] {
                                ui.label(format!("{:.2}", speed));
                            }
                            ui.end_row();
                        }
                    });
                });
            } else {
                ui.label("No Polar Data Loaded.");
            }
            
            ui.separator();
            ui.heading("Routing Step");
            
            ui.label(format!("Steps taken: {}", routing_state.fronts.len().saturating_sub(1)));
            
            if let Some(first_front) = routing_state.fronts.first() {
                if let Some(start_state) = first_front.first() {
                    let local_start = start_state.time.with_timezone(&chrono::Local);
                    ui.label(format!("Start UTC:   {}", start_state.time.format("%Y-%m-%d %H:%M")));
                    ui.label(format!("Start Local: {}", local_start.format("%Y-%m-%d %H:%M")));
                }
            }
            
            if let Some(front) = routing_state.fronts.last() {
                if let Some(state) = front.first() {
                    let local_time = state.time.with_timezone(&chrono::Local);
                    ui.label(format!("Front UTC:   {}", state.time.format("%Y-%m-%d %H:%M")));
                    ui.label(format!("Front Local: {}", local_time.format("%Y-%m-%d %H:%M")));
                }
                ui.label(format!("Active branch count: {}", front.len()));
            }
            
            ui.horizontal(|ui| {
                if ui.button("Step Forward").clicked() {
                    let current_front = routing_state.fronts.last().unwrap().clone();
                    
                    use crate::engine::models::{WindData, CurrentData};
                    let next_front = routing_state.router.step(
                        &current_front, 
                        &PhysicsModel::new(), 
                        &polar_data,
                        &land_mask,
                        |coord| wind_field.get_wind_at(coord).unwrap_or(WindData { u: 0.0, v: 0.0 }), 
                        |_| CurrentData { u: 0.0, v: 0.0 }
                    );
                    routing_state.fronts.push(next_front);
                }
                
                let play_label = if routing_state.is_playing { "Pause" } else { "Play" };
                if ui.button(play_label).clicked() {
                    routing_state.is_playing = !routing_state.is_playing;
                }
            });
            
            ui.separator();
            ui.heading("Map");
            ui.label("Rendering OpenSeaMap tiles and GRIB vectors.");
        });
}

// We imported the real `render_openseamap_system` from map.rs

fn camera_movement_system(
    mut q_camera: Query<(&Camera, &GlobalTransform, &mut Transform, &mut OrthographicProjection), With<Camera2d>>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut scroll_evr: EventReader<MouseWheel>,
    time: Res<Time>,
) {
    if let Ok((camera, camera_global_transform, mut transform, mut projection)) = q_camera.get_single_mut() {
        let mut pan = Vec2::ZERO;

        // Panning with Keyboard
        let pan_speed = 500.0 * projection.scale;
        if keyboard_input.pressed(KeyCode::ArrowUp) || keyboard_input.pressed(KeyCode::KeyW) {
            pan.y += pan_speed * time.delta_secs();
        }
        if keyboard_input.pressed(KeyCode::ArrowDown) || keyboard_input.pressed(KeyCode::KeyS) {
            pan.y -= pan_speed * time.delta_secs();
        }
        if keyboard_input.pressed(KeyCode::ArrowLeft) || keyboard_input.pressed(KeyCode::KeyA) {
            pan.x -= pan_speed * time.delta_secs();
        }
        if keyboard_input.pressed(KeyCode::ArrowRight) || keyboard_input.pressed(KeyCode::KeyD) {
            pan.x += pan_speed * time.delta_secs();
        }
        // Panning with Left click
        if mouse_buttons.pressed(MouseButton::Left) {
            for ev in mouse_motion_events.read() {
                 transform.translation.x -= ev.delta.x * projection.scale; // Delta is opposite direction of drag
                 transform.translation.y += ev.delta.y * projection.scale;
            }
        }
        
        transform.translation.x += pan.x;
        transform.translation.y += pan.y;

        // Zooming with Scroll Wheel (centered on cursor)
        if let Ok(window) = q_window.get_single() {
            for ev in scroll_evr.read() {
                let mut log_scale = projection.scale.ln();
                let zoom_delta = match ev.unit {
                    MouseScrollUnit::Line => -ev.y * 0.1,
                    MouseScrollUnit::Pixel => -ev.y * 0.005,
                };
                
                log_scale += zoom_delta;
                let new_scale = log_scale.exp();

                // Compute cursor position in world coordinates BEFORE zoom
                if let Some(cursor_position) = window.cursor_position() {
                    if let Ok(world_pos) = camera.viewport_to_world_2d(camera_global_transform, cursor_position) {
                        // After zoom, the same screen pixel should point to the same world_pos
                        // V' = (W - T') / S' => T' = W - V' * S'
                        // Since screen cursor `V'` (in centered logical coords) * S translates to world space delta:
                        let _ndc = (cursor_position / Vec2::new(window.width() as f32, window.height() as f32)) * 2.0 - Vec2::ONE;
                        let _ndc_to_world = transform.compute_matrix() * projection.get_clip_from_view().inverse();
                        
                        // We zoom first to find translation offset
                        let scale_ratio = new_scale / projection.scale;
                        let new_translation = world_pos - (world_pos - transform.translation.truncate()) * scale_ratio;
                        
                        transform.translation.x = new_translation.x;
                        transform.translation.y = new_translation.y;
                    }
                }
                
                projection.scale = new_scale;
            }
        }
    }
}

fn startup_load_grib(mut commands: Commands, mut polar_data: ResMut<PolarData>) {
    log::info!("Loading default IMOCA 60 polar...");
    *polar_data = PolarData::load_from_csv("data/imoca_60.csv");
    
    log::info!("Spawning background task to load GRIB data...");
    let thread_pool = IoTaskPool::get();
    let task = thread_pool.spawn(async move {
        let loader = GribLoader::new();
        match loader.load_wind_data("data/arpege_sample_small.grib2") {
            Ok(data) => data,
            Err(e) => {
                log::error!("Failed to load GRIB in background: {}", e);
                Vec::new()
            }
        }
    });
    commands.spawn(AsyncGribLoadTask(task));
}

fn handle_grib_load_task(
    mut commands: Commands,
    mut tasks_query: Query<(Entity, &mut AsyncGribLoadTask)>,
    mut wind_field: ResMut<WindField>,
) {
    for (entity, mut task) in &mut tasks_query {
        if let Some(data) = futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0)) {
            if !data.is_empty() {
                wind_field.chunks.clear();
                let count = data.len();
                for (coord, wind) in data {
                    wind_field.insert_point(coord, wind);
                }
                log::info!("Background GRIB loading complete. Chunked {} points into {} 1x1 degree cells.", count, wind_field.chunks.len());
            } else {
                log::warn!("Background GRIB loading returned empty data. Keeping last known valid GRIB data.");
            }
            commands.entity(entity).despawn();
        }
    }
}

fn routing_step_system(
    time: Res<Time>,
    mut routing_state: ResMut<RoutingState>,
    land_mask: Res<LandMask>,
    wind_field: Res<WindField>,
    polar_data: Res<PolarData>,
) {
    if !routing_state.is_playing { return; }
    
    routing_state.step_timer.tick(time.delta());
    if routing_state.step_timer.just_finished() {
        let current_front = routing_state.fronts.last().unwrap().clone();
        
        use crate::engine::models::{WindData, CurrentData};
        let next_front = routing_state.router.step(
            &current_front, 
            &PhysicsModel::new(), 
            &polar_data,
            &land_mask,
            |coord| wind_field.get_wind_at(coord).unwrap_or(WindData { u: 0.0, v: 0.0 }), 
            |_| CurrentData { u: 0.0, v: 0.0 }
        );
        routing_state.fronts.push(next_front);
    }
}
