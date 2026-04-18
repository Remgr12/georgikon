use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::client::input::{ActionState, GameAction};
use crate::client::world::GROUND_TOP_Y;
use crate::screens::Screen;
use crate::settings::Settings;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraState>()
            .add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(Screen::Gameplay), attach_gameplay_camera)
            .add_systems(OnExit(Screen::Gameplay), detach_gameplay_camera)
            .add_systems(Update, sync_cursor_grab_mode)
            .add_systems(Update, camera_update_system.run_if(in_state(Screen::Gameplay)))
            .add_systems(
                PostUpdate,
                prevent_camera_ground_clipping.run_if(in_state(Screen::Gameplay)),
            );
    }
}

#[derive(Component)]
pub struct SceneCamera;

#[derive(Resource, Default)]
pub struct CameraState {
    mode: CameraMode,
    target_mode: Option<CameraMode>,
    switch_timer: f32,
}

#[derive(PartialEq, Debug, Clone, Copy, Eq, Hash, Default)]
pub enum CameraMode {
    #[default]
    ThirdPerson,
    FirstPerson,
    Freefly,
    Tight,
}

const CAMERA_GROUND_CLEARANCE: f32 = 0.25;
const MODE_SWITCH_TIME: f32 = 0.1;
const LERP_ORI_RATE: f32 = 15.0;
const LERP_DIST_RATE: f32 = 6.5;
const LERP_FOV_RATE: f32 = 6.5;

#[derive(Component)]
struct CameraRig {
    target: Vec3,
    target_offset: Vec3,
    distance: f32,
    target_distance: f32,
    fov: f32,
    target_fov: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            target_offset: Vec3::new(0.0, 0.7, 0.25),
            distance: 6.0,
            target_distance: 6.0,
            fov: 1.1,
            target_fov: 1.1,
        }
    }
}

#[derive(Component)]
struct CameraController {
    rotation: Vec3,
    target_rotation: Vec3,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            rotation: Vec3::ZERO,
            target_rotation: Vec3::ZERO,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        SceneCamera,
        Camera3d::default(),
        IsDefaultUiCamera,
        CameraRig::default(),
        CameraController::default(),
    ));
}

fn attach_gameplay_camera(
    mut camera: Query<(&mut CameraRig, &mut CameraController), With<SceneCamera>>,
    settings: Res<Settings>,
) {
    let Ok((mut rig, mut ctrl)) = camera.single_mut() else {
        return;
    };

    rig.target_fov = settings.fov.to_radians();
    ctrl.rotation = Vec3::ZERO;
    ctrl.target_rotation = Vec3::ZERO;
}

fn detach_gameplay_camera(
    mut commands: Commands,
    camera: Query<Entity, With<SceneCamera>>,
) {
    if let Ok(cam) = camera.single() {
        commands.entity(cam).remove::<CameraRig>();
        commands.entity(cam).remove::<CameraController>();
    }
}

fn camera_update_system(
    time: Res<Time>,
    action_state: Res<ActionState>,
    mut camera: Query<
        (&mut CameraRig, &mut CameraController, &mut Transform),
        (With<SceneCamera>, Without<crate::client::player::Player>),
    >,
    mut camera_state: ResMut<CameraState>,
    player: Query<&Transform, With<crate::client::player::Player>>,
) {
    let Ok((mut rig, mut ctrl, mut tf)) = camera.single_mut() else {
        return;
    };

    let Ok(player_tf) = player.single() else {
        return;
    };

    if action_state.just_pressed(GameAction::CycleCameraMode) {
        let next_mode = match camera_state.mode {
            CameraMode::ThirdPerson => CameraMode::Tight,
            CameraMode::Tight => CameraMode::ThirdPerson,
            _ => CameraMode::ThirdPerson,
        };
        camera_state.target_mode = Some(next_mode);
        camera_state.switch_timer = MODE_SWITCH_TIME;
    }

    if let Some(target) = camera_state.target_mode {
        if camera_state.switch_timer > 0.0 {
            camera_state.switch_timer -= time.delta_secs();
        } else {
            camera_state.mode = target;
            camera_state.target_mode = None;
            apply_camera_mode(&mut rig, camera_state.mode);
        }
    }

    let dt = time.delta_secs();

    let mouse_delta = action_state.mouse_delta;
    if mouse_delta.length() > 0.0 {
        let sensitivity = Vec2::new(0.003, 0.002);
        ctrl.target_rotation.x -= mouse_delta.x * sensitivity.x;
        ctrl.target_rotation.y += mouse_delta.y * sensitivity.y;
        ctrl.target_rotation.y = ctrl.target_rotation.y.clamp(-1.5, 1.5);
    }

    if action_state.pressed(GameAction::ZoomIn) {
        rig.target_distance = (rig.target_distance - dt * 5.0).max(2.0);
    }
    if action_state.pressed(GameAction::ZoomOut) {
        rig.target_distance = (rig.target_distance + dt * 5.0).min(20.0);
    }

    let distance_lerp = smoothing_factor(LERP_DIST_RATE, dt);
    rig.distance = lerp_single(rig.distance, rig.target_distance, distance_lerp);

    let fov_lerp = smoothing_factor(LERP_FOV_RATE, dt);
    rig.fov = lerp_single(rig.fov, rig.target_fov, fov_lerp);

    let ori_lerp = smoothing_factor(LERP_ORI_RATE, dt);
    ctrl.rotation.x = lerp_angle(ctrl.rotation.x, ctrl.target_rotation.x, ori_lerp);
    ctrl.rotation.y = lerp_single(ctrl.rotation.y, ctrl.target_rotation.y, ori_lerp);

    let (rotation, distance) = (ctrl.rotation, rig.distance);
    let offset = rig.target_offset;

    let focus = player_tf.translation + Vec3::new(0.0, offset.y, 0.0);
    let offset_vec = compute_camera_offset(rotation, distance, offset);
    let cam_pos = focus + offset_vec;

    tf.translation = cam_pos;
    tf.look_at(focus, Vec3::Y);
}

fn sync_cursor_grab_mode(
    screen: Res<State<Screen>>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor_q.single_mut() else {
        return;
    };

    if *screen.get() == Screen::Gameplay {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    } else {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    }
}

fn apply_camera_mode(rig: &mut CameraRig, mode: CameraMode) {
    match mode {
        CameraMode::ThirdPerson => {
            rig.target_distance = 6.0;
            rig.target_offset = Vec3::new(0.0, 0.7, 0.25);
        }
        CameraMode::Tight => {
            rig.target_distance = 4.0;
            rig.target_offset = Vec3::new(0.0, 0.35, 0.15);
        }
        CameraMode::FirstPerson => {
            rig.target_distance = 0.1;
            rig.target_offset = Vec3::ZERO;
        }
        CameraMode::Freefly => {
            rig.target_distance = 0.1;
            rig.target_offset = Vec3::ZERO;
        }
    }
}

fn compute_camera_offset(rotation: Vec3, distance: f32, offset: Vec3) -> Vec3 {
    let (yaw, pitch) = (rotation.x, rotation.y);

    let forward = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let up = Vec3::Y;

    let base_offset = -forward * distance;
    let additional = right * offset.x + up * offset.y - forward * offset.z;

    base_offset + additional
}

fn lerp_single(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let diff = (b - a + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI)
        - std::f32::consts::PI;
    a + diff * t.clamp(0.0, 1.0)
}

fn smoothing_factor(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

fn prevent_camera_ground_clipping(
    mut camera: Query<&mut Transform, (With<SceneCamera>, Without<crate::client::player::Player>)>,
) {
    let Ok(mut tf) = camera.single_mut() else {
        return;
    };

    let min_camera_y = GROUND_TOP_Y + CAMERA_GROUND_CLEARANCE;
    if tf.translation.y < min_camera_y {
        tf.translation.y = min_camera_y;
    }
}
