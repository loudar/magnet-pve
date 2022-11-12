use bevy::prelude::*;
use crate::*;

pub struct CombatPlugin;

fn combat(
    mut commands: Commands,
    buttons: Res<Input<MouseButton>>,
    mut scoreboard: ResMut<Scoreboard>,
    mut player_query: Query<(&mut Sprite, &mut Transform), With<Player>>,
    mut enemy_query: Query<(&mut Sprite, &mut Transform, &mut Health, &Enemy, Entity), Without<Player>>,
)
{
    let (player_sprite, mut player_transform) = player_query.single_mut();
    let player_position = player_transform.translation.truncate();

    for (mut enemy_sprite, mut enemy_transform, mut enemy_health, enemy, entity) in enemy_query.iter_mut() {
        let enemy_position = enemy_transform.translation.truncate();
        let distance = player_position.distance(enemy_position);

        if distance < WEAPON_RADIUS && buttons.just_pressed(MouseButton::Left) {
            enemy_health.0 -= DAMAGE;
            if enemy_health.0 <= 0.0 {
                commands.entity(entity).despawn();
                scoreboard.score += 1;
            }
        }
    }
}