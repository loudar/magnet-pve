//! A simplified implementation of the classic game "Breakout".

use std::cmp::max;
use std::f64::consts::PI;
use bevy::{
    prelude::*,
    sprite::collide_aabb::{collide, Collision},
    time::FixedTimestep,
};
use bevy_simple_stat_bars::prelude::*;
use rand::prelude::*;

// Defines the amount of time that should elapse between each physics step.
const TIME_STEP: f32 = 1.0 / 60.0;

// These constants are defined in `Transform` units.
// Using the default 2D camera they correspond 1:1 with screen pixels.
const PLAYER_SIZE: Vec3 = Vec3::new(30.0, 30.0, 0.0);
const GAP_BETWEEN_PLAYER_AND_FLOOR: f32 = 60.0;
const PLAYER_SPEED: f32 = 300.0;
const ENEMY_SPEED: f32 = 150.0;
// How close can the player get to the wall
const PLAYER_PADDING: f32 = 10.0;

const WALL_THICKNESS: f32 = 10.0;
// x coordinates
const LEFT_WALL: f32 = -450.;
const RIGHT_WALL: f32 = 450.;
// y coordinates
const BOTTOM_WALL: f32 = -300.;
const TOP_WALL: f32 = 300.;

const ENEMY_SIZE: Vec2 = Vec2::new(20.0, 20.0);
// These values are exact
const GAP_BETWEEN_PLAYER_AND_ENEMIES: f32 = 270.0;

const SCOREBOARD_FONT_SIZE: f32 = 40.0;
const SCOREBOARD_TEXT_PADDING: Val = Val::Px(5.0);

const BACKGROUND_COLOR: Color = Color::rgb(0.05, 0.05, 0.05);
const ENEMY_COLOR: Color = Color::rgb(0.5, 0.5, 1.0);
const ENEMY_PULL_COLOR: Color = Color::rgb(1.0, 0.5, 0.5);
const ENEMY_PUSH_COLOR: Color = Color::rgb(0.5, 1.0, 0.5);
const WALL_COLOR: Color = Color::rgb(0.8, 0.8, 0.8);
const TEXT_COLOR: Color = Color::rgb(0.5, 0.5, 1.0);
const SCORE_COLOR: Color = Color::rgb(1.0, 0.5, 0.5);

const MAGNET_RADIUS: f32 = 200.0;
const MAGNET_FORCE: f32 = 50.0;

const ENEMY_COUNT: usize = 10; 
const VELOCITY_DRAG: f32 = 0.99;

const WEAPON_RADIUS: f32 = MAGNET_RADIUS / 2.0;
const DAMAGE: f32 = 10.0;

const PLAYER_HEALTH: f32 = 100.0;
const ENEMY_HEALTH: f32 = 10.0;

const EXPLOSION_SHEET: &str = "images/explo_a_sheet.png";
const EXPLOSION_LEN: usize = 16;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(Scoreboard { score: 0 })
        .insert_resource(ClearColor(BACKGROUND_COLOR))
        .add_startup_system(setup)
        .add_event::<MagnetPullEvent>()
        .add_event::<MagnetPushEvent>()
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(TIME_STEP as f64))
                .with_system(magnet.before(move_player))
                .with_system(play_magnet_sounds.after(magnet))
                .with_system(move_player.before(check_for_collisions))
                .with_system(apply_velocity.before(check_for_collisions))
                .with_system(combat.before(check_for_collisions))
                .with_system(check_for_collisions)
        )
        .add_system(update_scoreboard)
        .add_system(bevy::window::close_on_esc)
        .add_system(explosion_to_spawn_system)
        .add_system(explosion_animation_system)
        .run();
}

#[derive(Component)]
struct Player;

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

#[derive(Component)]
struct Collider;

#[derive(Default)]
struct MagnetPullEvent;

#[derive(Default)]
struct MagnetPushEvent;

#[derive(Component)]
struct Enemy;

#[derive(Component)]
struct Hp { current: i32, max: i32 }

#[derive(Component)]
pub struct Explosion;

#[derive(Component)]
pub struct ExplosionToSpawn(pub Vec3);

#[derive(Component)]
pub struct ExplosionTimer(pub Timer);

impl Default for ExplosionTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.05, true))
    }
}

struct MagnetPullSound(Handle<AudioSource>);
struct MagnetPushSound(Handle<AudioSource>);

struct ExplosionTexture(Handle<TextureAtlas>);

// This bundle is a collection of the components that define a "wall" in our game
#[derive(Bundle)]
struct WallBundle {
    // You can nest bundles inside of other bundles like this
    // Allowing you to compose their functionality
    #[bundle]
    sprite_bundle: SpriteBundle,
    collider: Collider,
}

enum WallLocation {
    Left,
    Right,
    Bottom,
    Top,
}

impl WallLocation {
    fn position(&self) -> Vec2 {
        match self {
            WallLocation::Left => Vec2::new(LEFT_WALL, 0.),
            WallLocation::Right => Vec2::new(RIGHT_WALL, 0.),
            WallLocation::Bottom => Vec2::new(0., BOTTOM_WALL),
            WallLocation::Top => Vec2::new(0., TOP_WALL),
        }
    }

    fn size(&self) -> Vec2 {
        let arena_height = TOP_WALL - BOTTOM_WALL;
        let arena_width = RIGHT_WALL - LEFT_WALL;
        // Make sure we haven't messed up our constants
        assert!(arena_height > 0.0);
        assert!(arena_width > 0.0);

        match self {
            WallLocation::Left | WallLocation::Right => {
                Vec2::new(WALL_THICKNESS, arena_height + WALL_THICKNESS)
            }
            WallLocation::Bottom | WallLocation::Top => {
                Vec2::new(arena_width + WALL_THICKNESS, WALL_THICKNESS)
            }
        }
    }
}

impl WallBundle {
    // This "builder method" allows us to reuse logic across our wall entities,
    // making our code easier to read and less prone to bugs when we change the logic
    fn new(location: WallLocation) -> WallBundle {
        WallBundle {
            sprite_bundle: SpriteBundle {
                transform: Transform {
                    // We need to convert our Vec2 into a Vec3, by giving it a z-coordinate
                    // This is used to determine the order of our sprites
                    translation: location.position().extend(0.0),
                    // The z-scale of 2D objects must always be 1.0,
                    // or their ordering will be affected in surprising ways.
                    // See https://github.com/bevyengine/bevy/issues/4149
                    scale: location.size().extend(1.0),
                    ..default()
                },
                sprite: Sprite {
                    color: WALL_COLOR,
                    ..default()
                },
                ..default()
            },
            collider: Collider,
        }
    }
}

// This resource tracks the game's score
struct Scoreboard {
    score: usize,
}

// Add the game's entities to our world
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    audio: Res<Audio>
)
{
    // Camera
    commands.spawn_bundle(Camera2dBundle::default());

    // Sound
    commands.insert_resource(MagnetPullSound(asset_server.load("sounds/magnet_pull.ogg")));
    commands.insert_resource(MagnetPushSound(asset_server.load("sounds/magnet_push.ogg")));
    
    commands.insert_resource(ExplosionTexture(
        texture_atlases.add(TextureAtlas::from_grid(
            asset_server.load(EXPLOSION_SHEET),
            Vec2::new(64.0, 64.0),
            EXPLOSION_LEN,
            EXPLOSION_LEN,
        ))
    ));

    audio.play_with_settings(asset_server.load("sounds/soundtrack.ogg"), PlaybackSettings::LOOP.with_volume(0.75));
    
    // Player
    let player_y = BOTTOM_WALL + GAP_BETWEEN_PLAYER_AND_FLOOR;
    let player = commands
        .spawn()
        .insert(Player)
        .insert(Hp { current: PLAYER_HEALTH as i32, max: PLAYER_HEALTH as i32 })
        .insert_bundle(SpriteBundle {
            transform: Transform {
                translation: Vec3::new(0.0, player_y, 0.0),
                scale: PLAYER_SIZE,
                ..default()
            },
            sprite: Sprite {
                custom_size: Option::from(Vec2::new(1.0, 1.0)),
                ..default()
            },
            texture: asset_server.load("images/player.png"),
            ..default()
        })
        .insert(Collider)
        .id();

    // Scoreboard
    commands.spawn_bundle(
        TextBundle::from_sections([
            TextSection::new(
                "Score: ",
                TextStyle {
                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                    font_size: SCOREBOARD_FONT_SIZE,
                    color: TEXT_COLOR,
                },
            ),
            TextSection::from_style(TextStyle {
                font: asset_server.load("fonts/FiraMono-Medium.ttf"),
                font_size: SCOREBOARD_FONT_SIZE,
                color: SCORE_COLOR,
            }),
        ])
            .with_style(Style {
                position_type: PositionType::Absolute,
                position: UiRect {
                    top: SCOREBOARD_TEXT_PADDING,
                    left: SCOREBOARD_TEXT_PADDING,
                    ..default()
                },
                ..default()
            }),
    );

    // Walls
    commands.spawn_bundle(WallBundle::new(WallLocation::Left));
    commands.spawn_bundle(WallBundle::new(WallLocation::Right));
    commands.spawn_bundle(WallBundle::new(WallLocation::Bottom));
    commands.spawn_bundle(WallBundle::new(WallLocation::Top));

    // Enemies
    for i in 0..ENEMY_COUNT {
        let enemy_position = Vec2::new(
            thread_rng().gen_range(LEFT_WALL..RIGHT_WALL),
            thread_rng().gen_range(BOTTOM_WALL..TOP_WALL),
        );
        
        let spritenum = thread_rng().gen_range(1..3);

        // enemy
        commands
            .spawn()
            .insert(Enemy)
            .insert(Hp { current: ENEMY_HEALTH as i32, max: ENEMY_HEALTH as i32 })
            .insert_bundle(SpriteBundle {
                sprite: Sprite {
                    color: ENEMY_COLOR,
                    custom_size: Option::from(Vec2::new(1.0, 1.0)),
                    flip_x: thread_rng().gen(),
                    flip_y: thread_rng().gen(),
                    ..default()
                },
                transform: Transform {
                    translation: enemy_position.extend(0.0),
                    scale: Vec3::new(ENEMY_SIZE.x, ENEMY_SIZE.y, 1.0),
                    rotation: Quat::from_rotation_z(thread_rng().gen_range(0.0..2.0 * PI) as f32),
                    ..default()
                },
                texture: asset_server.load(&format!("images/enemy_{}.png", spritenum.to_string())),
                ..default()
            })
            .insert(Velocity(Vec2::new(
                thread_rng().gen_range(-ENEMY_SPEED..ENEMY_SPEED),
                thread_rng().gen_range(-ENEMY_SPEED..ENEMY_SPEED),
            )))
            .insert(Collider);
    }
}

fn update_scoreboard(scoreboard: Res<Scoreboard>, mut query: Query<&mut Text>)
{
    let mut text = query.single_mut();
    text.sections[1].value = scoreboard.score.to_string();
}

fn point_in_radius(point: Vec2, center: Vec2, radius: f32) -> bool
{
    let distance = point.distance(center);
    distance < radius
}

fn combat(
    mut commands: Commands,
    buttons: Res<Input<MouseButton>>,
    keyboard_input: Res<Input<KeyCode>>,
    mut scoreboard: ResMut<Scoreboard>,
    mut player_query: Query<(&mut Sprite, &mut Transform), With<Player>>,
    mut enemy_query: Query<(&mut Sprite, &mut Transform, &mut Hp, &Enemy, Entity), Without<Player>>,
)
{
    let (player_sprite, mut player_transform) = player_query.single_mut();
    let player_position = player_transform.translation.truncate();

    for (mut enemy_sprite, mut enemy_transform, mut enemy_health, enemy, entity) in enemy_query.iter_mut() {
        let enemy_position = enemy_transform.translation.truncate();
        let distance = player_position.distance(enemy_position);

        if distance <= WEAPON_RADIUS &&
            (
                buttons.just_pressed(MouseButton::Left)
                    || keyboard_input.just_pressed(KeyCode::Space)
            ) {

            print!("Hit enemy!");
            enemy_health.current -= DAMAGE as i32;
            if enemy_health.current <= 0 {
                commands.entity(entity).despawn();
                scoreboard.score += 1;
                
                commands.spawn().insert(ExplosionToSpawn(enemy_transform.translation.clone()));
            }
        }
    }
}

fn explosion_to_spawn_system(
    mut commands: Commands,
    explosion_texture: Res<ExplosionTexture>,
    query: Query<(Entity, &ExplosionToSpawn)>,
) {
    for (explosion_spawn_entity, explosion_to_spawn) in query.iter() {
        // spawn the explosion sprite
        commands
            .spawn_bundle(SpriteSheetBundle {
                texture_atlas: explosion_texture.0.clone(),
                transform: Transform {
                    translation: explosion_to_spawn.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .insert(Explosion)
            .insert(ExplosionTimer::default());

        // despawn the explosionToSpawn
        commands.entity(explosion_spawn_entity).despawn();
    }
}

fn explosion_animation_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut ExplosionTimer, &mut TextureAtlasSprite), With<Explosion>>,
) {
    for (entity, mut timer, mut sprite) in query.iter_mut() {
        timer.0.tick(time.delta());
        if timer.0.finished() {
            sprite.index += 1; // move to next sprite cell
            if sprite.index >= EXPLOSION_LEN {
                commands.entity(entity).despawn()
            }
        }
    }
}

fn magnet(
    keyboard_input: Res<Input<KeyCode>>,
    mut query: Query<(&mut Sprite, &mut Transform), With<Player>>,
    mut enemy_query: Query<(&mut Sprite, &mut Transform, &mut Velocity, &Enemy), Without<Player>>,
    mut magnet_pull_events: EventWriter<MagnetPullEvent>,
    mut magnet_push_events: EventWriter<MagnetPushEvent>,
)
{
    let (mut player_sprite, mut player_transform) = query.single_mut();

    for (mut enemy_sprite, mut enemy_transform, mut enemy_velocity, maybe_enemy) in enemy_query.iter_mut() {
        enemy_sprite.color = ENEMY_COLOR;
    }
    
    if keyboard_input.just_pressed(KeyCode::Q) {
        magnet_pull_events.send(MagnetPullEvent);
    }
    
    if keyboard_input.just_pressed(KeyCode::E) {
        magnet_push_events.send(MagnetPushEvent);
    }
    
    if keyboard_input.pressed(KeyCode::Q) {
        player_sprite.flip_y = true;
        for (mut enemy_sprite, mut enemy_transform, mut enemy_velocity, maybe_enemy) in enemy_query.iter_mut() {
            pull_push_enemy(&mut player_transform, &mut enemy_sprite, &mut enemy_transform, &mut enemy_velocity, false);
        }
    } else {
        player_sprite.flip_y = false;
    }
    
    if keyboard_input.pressed(KeyCode::E) {
        for (mut enemy_sprite, mut enemy_transform, mut enemy_velocity, maybe_enemy) in enemy_query.iter_mut() {
            pull_push_enemy(&mut player_transform, &mut enemy_sprite, &mut enemy_transform, &mut enemy_velocity, true);
        }
    }
}

fn pull_push_enemy(
    player_transform: &mut Transform,
    enemy_sprite: &mut Sprite,
    enemy_transform: &mut Transform,
    enemy_velocity: &mut Velocity,
    is_push: bool
)
{
    if !point_in_radius(
        enemy_transform.translation.truncate(),
        player_transform.translation.truncate(),
        MAGNET_RADIUS,
    ) {
        return;
    }

    let direction;
    if is_push {
        direction = enemy_transform.translation - player_transform.translation;
    } else {
        direction = player_transform.translation - enemy_transform.translation;
    }
    let distance = direction.length();
    let normalized_direction = direction.normalize();

    let additional_speed = MAGNET_FORCE * (MAGNET_RADIUS / distance);
    let target_speed = ENEMY_SPEED + additional_speed;
    let target_x = normalized_direction.x * target_speed * VELOCITY_DRAG;
    let target_y = normalized_direction.y * target_speed * VELOCITY_DRAG;
    let mut moved = false;
    if enemy_transform.translation.x + target_x > LEFT_WALL && enemy_transform.translation.x + target_x < RIGHT_WALL {
        enemy_velocity.x = target_x;
        moved = true;
    }
    if enemy_transform.translation.y + target_y > BOTTOM_WALL && enemy_transform.translation.y + target_y < TOP_WALL {
        enemy_velocity.y = target_y;
        moved = true;
    }
    
    if !moved {
        return;
    }
    if is_push {
        enemy_sprite.color = ENEMY_PUSH_COLOR;
    } else {
        enemy_sprite.color = ENEMY_PULL_COLOR;
    }
}

fn move_player(
    keyboard_input: Res<Input<KeyCode>>,
    mut query: Query<(&mut Sprite, &mut Transform), With<Player>>,
)
{
    let (mut player_sprite, mut player_transform) = query.single_mut();
    let mut direction = Vec2::ZERO;

    if keyboard_input.pressed(KeyCode::Left) {
        direction.x -= 1.0;
    }

    if keyboard_input.pressed(KeyCode::Right) {
        direction.x += 1.0;
    }

    if keyboard_input.pressed(KeyCode::Up) {
        direction.y += 1.0;
    }

    if keyboard_input.pressed(KeyCode::Down) {
        direction.y -= 1.0;
    }

    let new_player_pos_x = player_transform.translation.x + direction.x * PLAYER_SPEED * TIME_STEP;
    let new_player_pos_y = player_transform.translation.y + direction.y * PLAYER_SPEED * TIME_STEP;

    // Update the player position,
    // making sure it doesn't cause the player to leave the arena
    let left_bound = LEFT_WALL + WALL_THICKNESS / 2.0 + PLAYER_SIZE.x / 2.0 + PLAYER_PADDING;
    let right_bound = RIGHT_WALL - WALL_THICKNESS / 2.0 - PLAYER_SIZE.x / 2.0 - PLAYER_PADDING;
    let bottom_bound = BOTTOM_WALL + WALL_THICKNESS / 2.0 + PLAYER_SIZE.y / 2.0 + PLAYER_PADDING;
    let top_bound = TOP_WALL - WALL_THICKNESS / 2.0 - PLAYER_SIZE.y / 2.0 - PLAYER_PADDING;

    player_transform.translation.x = new_player_pos_x.clamp(left_bound, right_bound);
    player_transform.translation.y = new_player_pos_y.clamp(bottom_bound, top_bound);
}

fn move_enemies_to_player(
    mut player_query: Query<(&mut Sprite, &mut Transform), With<Player>>,
    mut query: Query<(&mut Sprite, &mut Transform, &mut Velocity, &Enemy), Without<Player>>,
)
{
    let (mut player_sprite, mut player_transform) = player_query.single_mut();
    for (mut enemy_sprite, mut enemy_transform, mut enemy_velocity, maybe_enemy) in query.iter_mut() {
        let direction = player_transform.translation - enemy_transform.translation;
        let distance = direction.length();
        let normalized_direction = direction.normalize();

        let target_speed = ENEMY_SPEED;
        let target_x = normalized_direction.x * target_speed * VELOCITY_DRAG;
        let target_y = normalized_direction.y * target_speed * VELOCITY_DRAG;
        if enemy_transform.translation.x + target_x > LEFT_WALL && enemy_transform.translation.x + target_x < RIGHT_WALL {
            enemy_velocity.x = target_x;
        }
        if enemy_transform.translation.y + target_y > BOTTOM_WALL && enemy_transform.translation.y + target_y < TOP_WALL {
            enemy_velocity.y = target_y;
        }
    }
}

fn apply_velocity(mut query: Query<(&mut Transform, &Velocity)>)
{
    for (mut transform, velocity) in &mut query {
        transform.translation.x += velocity.x * TIME_STEP;
        transform.translation.y += velocity.y * TIME_STEP;
    }
}

// check collisions for enemies with walls
fn check_for_collisions(
    mut commands: Commands,
    mut scoreboard: ResMut<Scoreboard>,
    mut enemy_query: Query<(Entity, &mut Velocity, &Transform, &Collider), With<Enemy>>,
    collider_query: Query<(Entity, &Transform), With<Collider>>,
)
{
    for (enemy_entity, mut enemy_velocity, enemy_transform, enemy_collider) in enemy_query.iter_mut() {
        for (wall_entity, wall_transform) in collider_query.iter() {
            let collision = collide(
                enemy_transform.translation,
                ENEMY_SIZE,
                wall_transform.translation,
                wall_transform.scale.truncate(),
            );
            
            if let Some(collision) = collision {                
                // reflect the ball when it collides
                let mut reflect_x = false;
                let mut reflect_y = false;

                // only reflect if the ball's velocity is going in the opposite direction of the
                // collision
                match collision {
                    Collision::Left => reflect_x = enemy_velocity.x > 0.0,
                    Collision::Right => reflect_x = enemy_velocity.x < 0.0,
                    Collision::Top => reflect_y = enemy_velocity.y < 0.0,
                    Collision::Bottom => reflect_y = enemy_velocity.y > 0.0,
                    Collision::Inside => { /* do nothing */ }
                }

                // reflect velocity on the x-axis if we hit something on the x-axis
                if reflect_x {
                    enemy_velocity.x = -enemy_velocity.x;
                }

                // reflect velocity on the y-axis if we hit something on the y-axis
                if reflect_y {
                    enemy_velocity.y = -enemy_velocity.y;
                }
            }
        }
    }
}

fn play_magnet_sounds(
    magnet_pull_events: EventReader<MagnetPullEvent>,
    magnet_push_events: EventReader<MagnetPushEvent>,
    audio: Res<Audio>,
    pull_sound: Res<MagnetPullSound>,
    push_sound: Res<MagnetPushSound>,
)
{
    if !magnet_pull_events.is_empty() {
        magnet_pull_events.clear();
        audio.play(pull_sound.0.clone());
    }
    if !magnet_push_events.is_empty() {
        magnet_push_events.clear();
        audio.play(push_sound.0.clone());
    }
}