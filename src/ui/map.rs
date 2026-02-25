use bevy::{prelude::*, tasks::{IoTaskPool, Task}};
use std::collections::HashSet;
use std::fs;
use futures_lite::future;
use image::{load_from_memory_with_format, ImageFormat};
use std::f64::consts::PI;

use crate::engine::models::{Coordinate, WindField};


// OpenStreetMap base tile URL format: https://tile.openstreetmap.org/{z}/{x}/{y}.png
const BASE_URL: &str = "https://tile.openstreetmap.org";

pub const TILE_SIZE: f32 = 256.0;

/// Projects a geographical coordinate to Web Mercator pixel coordinates mapped to Zoom level 1 space
pub fn project_mercator(coord: &Coordinate, _zoom: u8) -> Vec2 {
    let lat_rad = coord.lat.to_radians();
    let n = 2.0; // Force space to be equivalent to zoom 1 (512 total width)
    
    let x = (coord.lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / PI) / 2.0 * n;
    
// Convert to Bevy coordinates (Y is up in Bevy, Y is down in Mercator)
    Vec2::new((x * TILE_SIZE as f64) as f32, -(y * TILE_SIZE as f64) as f32)
}

/// Inverts project_mercator to get Lat/Lon from Bevy world coordinates
pub fn inverse_project_mercator(pos: Vec2) -> Coordinate {
    let n = 2.0;
    let lon = (pos.x as f64 * 360.0 / (n * TILE_SIZE as f64)) - 180.0;
    
    let y_merc = -pos.y as f64 / (n * TILE_SIZE as f64 / 2.0); // 0 to 1
    let lat_rad = 2.0 * ((PI * (1.0 - y_merc)).exp().atan()) - PI/2.0;
    
    Coordinate {
        lat: lat_rad.to_degrees(),
        lon,
    }
}

/// A component representing an active tile download task
#[derive(Component)]
pub struct TileDownloadTask(Task<(String, Option<(u8, u32, u32, Vec<u8>)>)>);

/// A marker component for a spawned map tile
#[derive(Component)]
pub struct MapTile {
    pub zoom: u8,
    pub tile_id: String,
}

/// Keeps track of which tiles are currently downloading or have been loaded
#[derive(Resource, Default)]
pub struct TileManager {
    pub loaded_tiles: HashSet<String>,
    pub downloading_tiles: HashSet<String>,
}

pub fn render_openseamap_system(
    mut commands: Commands,
    mut tile_manager: ResMut<TileManager>,
    mut images: ResMut<Assets<Image>>,
    mut tasks_query: Query<(Entity, &mut TileDownloadTask)>,
    q_camera: Query<(&Camera, &Transform, &OrthographicProjection), With<Camera2d>>,
    q_window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    q_tiles: Query<(Entity, &MapTile)>,
) {
    // 1. Process completed tile downloads
    for (entity, mut task) in &mut tasks_query {
        if let Some((tile_id, result)) = future::block_on(future::poll_once(&mut task.0)) {
            tile_manager.downloading_tiles.remove(&tile_id);
            if let Some((zoom, x, y, bytes)) = result {
                
                if let Ok(dynamic_image) = load_from_memory_with_format(&bytes, ImageFormat::Png) {
                    let rgba_image = dynamic_image.to_rgba8();
                    let (width, height) = rgba_image.dimensions();
                    
                    let image = Image::new(
                        bevy::render::render_resource::Extent3d {
                            width, height, depth_or_array_layers: 1,
                        },
                        bevy::render::render_resource::TextureDimension::D2,
                        rgba_image.into_raw(),
                        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                        bevy::asset::RenderAssetUsages::default(),
                    );
                    
                    let image_handle = images.add(image);
                    
                    // We forced the world coordinate space to zoom = 1 (World Width = 512.0)
                    let world_tile_size = 512.0 / f32::powi(2.0, zoom as i32);
                    
                    let center_x = (x as f32 + 0.5) * world_tile_size;
                    let center_y = -((y as f32 + 0.5) * world_tile_size);
                    
                    commands.spawn((
                        Sprite {
                            image: image_handle,
                            custom_size: Some(Vec2::new(world_tile_size, world_tile_size)),
                            ..default()
                        },
                        Transform::from_xyz(center_x, center_y, -20.0 + (zoom as f32) * 0.1),
                        MapTile {
                            zoom,
                            tile_id: tile_id.clone(),
                        },
                    ));
                    
                    tile_manager.loaded_tiles.insert(tile_id);
                } else {
                    log::warn!("Failed to decode tile image for: {}", tile_id);
                }
            } else {
                log::warn!("Tile download failed for: {}", tile_id);
            }
            commands.entity(entity).despawn();
        }
    }
    
    // 2. Trigger new downloads based on camera bounds
    let mut target_zoom: u8 = 4;
    let mut min_world_x = 0.0;
    let mut max_world_x = 512.0;
    let mut min_world_y = -512.0;
    let mut max_world_y = 0.0;
    
    if let Ok((_, transform, proj)) = q_camera.get_single() {
        let (window_width, window_height) = if let Ok(window) = q_window.get_single() {
            (window.width(), window.height())
        } else {
            (1000.0, 800.0)
        };
        
        // Base coordinate width is 512.0. If the visible window is viewing `window_width * proj.scale`
        // Then Zoom level is log2(512.0 / visible_world_width)
        let visible_world_width = window_width * proj.scale;
        
        // Bias target_zoom by +1.0 to ensure text and lines stay crisp (High-DPI feel)
        let z_calc = (2.0 - proj.scale.log2()).round() as i32;
        target_zoom = z_calc.clamp(1, 18) as u8;
        
        min_world_x = transform.translation.x - (visible_world_width / 2.0);
        max_world_x = transform.translation.x + (visible_world_width / 2.0);
        min_world_y = transform.translation.y - (window_height * proj.scale / 2.0);
        max_world_y = transform.translation.y + (window_height * proj.scale / 2.0);
    }
    
    let world_tile_size = 512.0 / f32::powi(2.0, target_zoom as i32);
    let max_index = (1 << target_zoom) - 1;

    let x_min = (min_world_x / world_tile_size).floor() as i32;
    let x_max = (max_world_x / world_tile_size).ceil() as i32;
    // Y map is inverted (negative bounding)
    let y_min = (-max_world_y / world_tile_size).floor() as i32;
    let y_max = (-min_world_y / world_tile_size).ceil() as i32;
    
    let x_min = x_min.clamp(0, max_index) as u32;
    let x_max = x_max.clamp(0, max_index) as u32;
    let y_min = y_min.clamp(0, max_index) as u32;
    let y_max = y_max.clamp(0, max_index) as u32;
    
    for x in x_min..=x_max {
        for y in y_min..=y_max {
            let zoom = target_zoom;
            let tile_id = format!("{}/{}/{}", zoom, x, y);

            if !tile_manager.loaded_tiles.contains(&tile_id) && !tile_manager.downloading_tiles.contains(&tile_id) {
                // To avoid massive downloading queues holding up the CPU, limit downloads
                if tile_manager.downloading_tiles.len() > 30 {
                    return;
                }
                
                tile_manager.downloading_tiles.insert(tile_id.clone());
                
                let url = format!("{}/{}/{}/{}.png", BASE_URL, zoom, x, y);
                let thread_pool = IoTaskPool::get();
                let task = thread_pool.spawn(async move {
                    let cache_dir = format!("data/tiles/{}", zoom);
                    let cache_file = format!("{}/{}_{}.png", cache_dir, x, y);
                    
                    if let Ok(bytes) = fs::read(&cache_file) {
                        return (tile_id, Some((zoom, x, y, bytes)));
                    }
                    
                    let client = match reqwest::blocking::Client::builder().user_agent("AIWeatherRouting/0.1").build() {
                        Ok(c) => c,
                        Err(_) => return (tile_id, None),
                    };
                    
                    match client.get(&url).send() {
                        Ok(response) => {
                            if let Ok(bytes) = response.bytes() {
                                let vec = bytes.to_vec();
                                let _ = fs::create_dir_all(&cache_dir);
                                let _ = fs::write(&cache_file, &vec);
                                (tile_id, Some((zoom, x, y, vec)))
                            } else {
                                (tile_id, None)
                            }
                        }
                        Err(_) => (tile_id, None)
                    }
                });
                commands.spawn(TileDownloadTask(task));
            }
        }
    }
    
    // 3. Cleanup old/invisible tiles (keep a range to prevent flickering)
    for (entity, tile) in &q_tiles {
        let diff = (tile.zoom as i32 - target_zoom as i32).abs();
        if diff > 1 {
            commands.entity(entity).despawn();
            tile_manager.loaded_tiles.remove(&tile.tile_id);
        }
    }
}

/// Renders a Latitude / Longitude grid
pub fn render_grid_system(
    mut commands: Commands,
    mut gizmos: Gizmos,
    mut grid_labels_spawned: Local<bool>,
) {
    let zoom = 1;
    let color = Color::srgba(0.5, 0.5, 0.5, 0.3); // Transparent gray
    let text_color = Color::srgba(0.8, 0.8, 0.8, 0.8);
    let font_size = 8.0;

    // Draw Longitude lines (Meridians)
    // -180 to 180 every 10 degrees
    let mut lon = -180.0;
    while lon <= 180.0 {
        let p_top = project_mercator(&Coordinate { lat: 85.0, lon }, zoom);
        let p_bottom = project_mercator(&Coordinate { lat: -85.0, lon }, zoom);
        gizmos.line_2d(p_top, p_bottom, color);
        
        if !*grid_labels_spawned {
            // Spawn a label at the equator for this longitude
            let p_label = project_mercator(&Coordinate { lat: 0.0, lon }, zoom);
            commands.spawn((
                Text2d::new(format!("{}°", lon)),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(text_color),
                Transform::from_xyz(p_label.x, p_label.y, 1.0),
            ));
        }
        
        lon += 10.0;
    }

    // Draw Latitude lines (Parallels)
    // -80 to 80 every 10 degrees (mercator goes to infinity at 90)
    let mut lat = -80.0;
    while lat <= 80.0 {
        let p_left = project_mercator(&Coordinate { lat, lon: -180.0 }, zoom);
        let p_right = project_mercator(&Coordinate { lat, lon: 180.0 }, zoom);
        gizmos.line_2d(p_left, p_right, color);
        
        if !*grid_labels_spawned {
            // Spawn a label at the prime meridian for this latitude
            let p_label = project_mercator(&Coordinate { lat, lon: 0.0 }, zoom);
            commands.spawn((
                Text2d::new(format!("{}°", lat)),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(text_color),
                Transform::from_xyz(p_label.x, p_label.y, 1.0),
            ));
        }
        
        lat += 10.0;
    }
    
    *grid_labels_spawned = true;
}

/// System to draw mathematical wind barbules using Bevy Gizmos over the Mercator projected grid
pub fn render_wind_barbules_system(
    wind_field: Res<WindField>,
    mut gizmos: Gizmos,
    q_camera: Query<(&Camera, &Transform, &OrthographicProjection), With<Camera2d>>,
    q_window: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let zoom = 1;
    
    // 1. Calculate visible Lat/Lon bounds
    let mut min_coord = Coordinate { lat: -80.0, lon: -180.0 };
    let mut max_coord = Coordinate { lat: 80.0, lon: 180.0 };
    
    if let Ok((_, transform, proj)) = q_camera.get_single() {
        if let Ok(window) = q_window.get_single() {
            let half_width = (window.width() * proj.scale) / 2.0;
            let half_height = (window.height() * proj.scale) / 2.0;
            
            let bottom_left = inverse_project_mercator(Vec2::new(transform.translation.x - half_width, transform.translation.y - half_height));
            let top_right = inverse_project_mercator(Vec2::new(transform.translation.x + half_width, transform.translation.y + half_height));
            
            min_coord.lon = bottom_left.lon.max(-180.0);
            max_coord.lon = top_right.lon.min(180.0);
            min_coord.lat = bottom_left.lat.max(-85.0);
            max_coord.lat = top_right.lat.min(85.0);
        }
    }

    // Adjust grid density based on camera scale? For now, fixed 0.5 degree grid
    // If the window is zoomed out, we want a coarser grid.
    let mut grid_step = 1.0;
    if let Ok((_, _, proj)) = q_camera.get_single() {
        if proj.scale < 0.05 { grid_step = 0.25; }
        else if proj.scale < 0.2 { grid_step = 0.5; }
        else if proj.scale > 1.0 { grid_step = 2.0; }
    }
    
    // Snap bounds to grid
    let start_lon = (min_coord.lon / grid_step).floor() * grid_step;
    let end_lon = (max_coord.lon / grid_step).ceil() * grid_step;
    let start_lat = (min_coord.lat / grid_step).floor() * grid_step;
    let end_lat = (max_coord.lat / grid_step).ceil() * grid_step;

    // Scale for the entire barbule drawing. 
    let stem_len = 1.0; 
    let ms_to_knots = 1.943844;

    let mut lon = start_lon;
    while lon <= end_lon {
        let mut lat = start_lat;
        while lat <= end_lat {
            let coord = Coordinate { lat, lon };
            if let Some(wind) = wind_field.get_wind_at(&coord) {
                // Only draw points that have significant wind to avoid cluttering 0 values
                let speed_kts = wind.speed() * ms_to_knots as f32;
                if speed_kts < 2.0 { 
                    lat += grid_step;
                    continue; 
                }

                let color = if speed_kts < 5.0 {
                    Color::srgba(0.7, 0.7, 1.0, 0.8) // Light blue
                } else if speed_kts < 15.0 {
                    Color::srgba(0.0, 0.5, 1.0, 0.8) // Blue
                } else if speed_kts < 25.0 {
                    Color::srgba(0.0, 1.0, 0.0, 0.8) // Green
                } else if speed_kts < 35.0 {
                    Color::srgba(1.0, 1.0, 0.0, 0.8) // Yellow
                } else if speed_kts < 45.0 {
                    Color::srgba(1.0, 0.5, 0.0, 0.8) // Orange
                } else {
                    Color::srgba(1.0, 0.0, 0.0, 0.8) // Red
                };

                // Project coordinate to screen pixel location
                let origin = project_mercator(&coord, zoom);
            
                // The stem points TOWARDS where the wind is coming FROM.
                let wind_vec = Vec2::new(-wind.u, -wind.v).normalize_or_zero();
                
                if wind_vec != Vec2::ZERO {
                    let tail = origin + wind_vec * stem_len;
                    
                    // Draw the main stem
                    gizmos.line_2d(origin, tail, color);
                    
                    // Draw a dot at the station
                    gizmos.circle_2d(origin, 0.2, color);
                    
                    // Calculate the normal vector for barbs
                    let normal = Vec2::new(wind_vec.y, -wind_vec.x);
                    
                    // Process barbs
                    let mut speed = speed_kts.round() as i32;
                    let pennants = speed / 50;
                    speed %= 50;
                    let full_barbs = speed / 10;
                    speed %= 10;
                    let half_barbs = speed / 5;
                    
                    let mut curr_tail = tail;
                    let barb_spacing = wind_vec * (stem_len * 0.15); 
                    let barb_len = stem_len * 0.4;
                    
                    for _ in 0..pennants {
                        let p1 = curr_tail;
                        let p2 = curr_tail + normal * barb_len;
                        let p3 = curr_tail - barb_spacing * 2.0;
                        gizmos.line_2d(p1, p2, color);
                        gizmos.line_2d(p2, p3, color);
                        curr_tail -= barb_spacing * 2.5; 
                    }
                    
                    for _ in 0..full_barbs {
                        let tip = curr_tail + normal * barb_len;
                        let angled_tip = tip + wind_vec * (barb_len * 0.2); 
                        gizmos.line_2d(curr_tail, angled_tip, color);
                        curr_tail -= barb_spacing;
                    }
                    
                    if half_barbs > 0 {
                        if pennants == 0 && full_barbs == 0 {
                            curr_tail -= barb_spacing * 0.5;
                        }
                        let tip = curr_tail + normal * (barb_len * 0.5); 
                        let angled_tip = tip + wind_vec * (barb_len * 0.1);
                        gizmos.line_2d(curr_tail, angled_tip, color);
                    }
                }
            }
            lat += grid_step;
        }
        lon += grid_step;
    }
}

/// Renders the isochrone points from the routing state
pub fn render_isochrones_system(
    routing_state: Res<crate::engine::router::RoutingState>,
    mut gizmos: Gizmos,
    q_camera: Query<&OrthographicProjection, With<Camera2d>>,
) {
    let zoom = 1;
    let scale = q_camera.get_single().map(|p| p.scale).unwrap_or(1.0);
    
    // Render Destination (Red)
    let dest_px = project_mercator(&routing_state.router.destination, zoom);
    gizmos.circle_2d(dest_px, 1.0 * scale, Color::srgba(1.0, 0.0, 0.0, 1.0));
    
    // Render Start (Green)
    let start_px = project_mercator(&routing_state.router.start, zoom);
    gizmos.circle_2d(start_px, 1.0 * scale, Color::srgba(0.0, 1.0, 0.0, 1.0));

    let last_idx = routing_state.fronts.len().saturating_sub(1);

    for (step_idx, front) in routing_state.fronts.iter().enumerate() {
        // Only draw every few points for historical fronts to avoid lag
        let is_latest = step_idx == last_idx;
        let stride = if is_latest { 1 } else { 5 };
        
        // Dynamic color based on step index using HSL for a nice gradient/cycle
        let hue = (step_idx as f32 * 20.0) % 360.0;
        let saturation = 0.8;
        let lightness = if is_latest { 0.6 } else { 0.4 };
        let alpha = if is_latest { 1.0 } else { 0.3 };
        
        let color = Color::hsla(hue, saturation, lightness, alpha);
        
        for (i, state) in front.iter().enumerate() {
            if i % stride != 0 { continue; }
            let pos_px = project_mercator(&state.position, zoom);
            // 2.0 world units * scale = 2 pixels radius = 4 pixels diameter
            let size = if is_latest { 2.0 * scale  } else { 1.5 * scale };
            // Draw as a filled circle (dot)
            gizmos.circle_2d(pos_px, size / 5.0, color);
        }
    }
}
