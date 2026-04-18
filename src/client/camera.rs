use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::ui::IsDefaultUiCamera;
use bevy_third_person_camera::{
    CameraSyncSet, ThirdPersonCamera, ThirdPersonCameraPlugin, ThirdPersonCameraTarget, Zoom,
};

use crate::client::world::GROUND_TOP_Y;
use crate::screens::Screen;
use crate::settings::Settings;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ThirdPersonCameraPlugin)
            .add_systems(Startup, spawn_camera)
            .add_systems(PostStartup, attach_initial_gameplay_camera)
            .add_systems(OnEnter(Screen::Gameplay), attach_third_person_camera)
            .add_systems(OnExit(Screen::Gameplay), detach_third_person_camera)
            .add_systems(
                PostUpdate,
                (
                    smooth_camera_motion,
                    prevent_camera_ground_clipping,
                    enforce_upright_camera,
                )
                    .chain()
                    .after(CameraSyncSet)
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(Screen::Gameplay)),
            );
    }
}

/// Marks the primary scene camera entity.
#[derive(Component)]
pub struct SceneCamera;

fn spawn_camera(mut commands: Commands) {
    // Spawn without ThirdPersonCamera – it is attached in attach_third_person_camera.
    commands.spawn((SceneCamera, Camera3d::default(), IsDefaultUiCamera));
}

fn attach_initial_gameplay_camera(
    mut commands: Commands,
    screen: Res<State<Screen>>,
    settings: Res<Settings>,
    camera: Query<Entity, (With<SceneCamera>, Without<ThirdPersonCamera>)>,
) {
    if screen.get() != &Screen::Gameplay {
        return;
    }

    let Ok(cam) = camera.single() else {
        debug!("attach_initial_gameplay_camera: camera entity not found or already has TPC");
        return;
    };

    insert_gameplay_camera_components(&mut commands, cam, &settings);
    debug!("attach_initial_gameplay_camera: attached third-person camera");
}

fn attach_third_person_camera(
    mut commands: Commands,
    settings: Res<Settings>,
    camera: Query<Entity, (With<SceneCamera>, Without<ThirdPersonCamera>)>,
) {
    let Ok(cam) = camera.single() else {
        debug!("attach_third_person_camera: camera entity not found or already has TPC");
        return;
    };

    insert_gameplay_camera_components(&mut commands, cam, &settings);
}

fn insert_gameplay_camera_components(commands: &mut Commands, cam: Entity, settings: &Settings) {
    commands.entity(cam).insert((
        ThirdPersonCamera {
            zoom: Zoom::new(3.0, 12.0),
            sensitivity: Vec2::new(4.5, 4.5),
            cursor_lock_key: KeyCode::Tab,
            offset_enabled: false,
            ..default()
        },
        CameraSmoothingState::default(),
        Projection::from(PerspectiveProjection {
            fov: settings.fov.to_radians(),
            ..Default::default()
        }),
    ));
}

#[derive(Component, Default)]
struct CameraSmoothingState {
    initialized: bool,
    translation: Vec3,
    rotation: Quat,
}

const CAMERA_POSITION_SMOOTHING: f32 = 30.0;
const CAMERA_ROTATION_SMOOTHING: f32 = 34.0;
const CAMERA_TRANSLATION_DEADZONE: f32 = 0.012;
const CAMERA_ROTATION_DEADZONE: f32 = 0.003;
const CAMERA_GROUND_CLEARANCE: f32 = 0.25;

fn smooth_camera_motion(
    time: Res<Time>,
    mut camera: Query<
        (&mut Transform, &mut CameraSmoothingState),
        (With<SceneCamera>, With<ThirdPersonCamera>),
    >,
) {
    let Ok((mut camera_tf, mut smoothing)) = camera.single_mut() else {
        return;
    };

    let target_translation = camera_tf.translation;
    let target_rotation = camera_tf.rotation;

    if !smoothing.initialized {
        smoothing.translation = target_translation;
        smoothing.rotation = target_rotation;
        smoothing.initialized = true;
        return;
    }

    let dt = time.delta_secs();
    let translation_delta = smoothing.translation.distance(target_translation);
    let rotation_delta = smoothing.rotation.angle_between(target_rotation).abs();

    if translation_delta < CAMERA_TRANSLATION_DEADZONE && rotation_delta < CAMERA_ROTATION_DEADZONE
    {
        camera_tf.translation = smoothing.translation;
        camera_tf.rotation = smoothing.rotation;
        return;
    }

    let pos_alpha = 1.0 - (-CAMERA_POSITION_SMOOTHING * dt).exp();
    let rot_alpha = 1.0 - (-CAMERA_ROTATION_SMOOTHING * dt).exp();

    smoothing.translation = smoothing.translation.lerp(target_translation, pos_alpha);
    smoothing.rotation = smoothing.rotation.slerp(target_rotation, rot_alpha);

    camera_tf.translation = smoothing.translation;
    camera_tf.rotation = smoothing.rotation;
}

fn prevent_camera_ground_clipping(
    target: Query<&Transform, With<ThirdPersonCameraTarget>>,
    mut camera: Query<
        &mut Transform,
        (
            With<SceneCamera>,
            With<ThirdPersonCamera>,
            Without<ThirdPersonCameraTarget>,
        ),
    >,
) {
    let Ok(target_tf) = target.single() else {
        return;
    };
    let Ok(mut camera_tf) = camera.single_mut() else {
        return;
    };

    let min_camera_y = GROUND_TOP_Y + CAMERA_GROUND_CLEARANCE;
    if camera_tf.translation.y < min_camera_y {
        camera_tf.translation.y = min_camera_y;
        camera_tf.look_at(target_tf.translation, Vec3::Y);
    }
}

fn enforce_upright_camera(
    mut camera: Query<&mut Transform, (With<SceneCamera>, With<ThirdPersonCamera>)>,
) {
    let Ok(mut camera_tf) = camera.single_mut() else {
        return;
    };

    let (yaw, pitch, _) = camera_tf.rotation.to_euler(EulerRot::YXZ);
    camera_tf.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}

fn detach_third_person_camera(
    mut commands: Commands,
    camera: Query<Entity, With<ThirdPersonCamera>>,
) {
    if let Ok(cam) = camera.single() {
        commands
            .entity(cam)
            .remove::<(ThirdPersonCamera, CameraSmoothingState)>();
    }
}
