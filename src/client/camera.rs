use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::ui::IsDefaultUiCamera;
use bevy_third_person_camera::{
    CameraSyncSet, ThirdPersonCamera, ThirdPersonCameraPlugin, ThirdPersonCameraTarget, Zoom,
};

use crate::client::input::{ActionState, GameAction};
use crate::client::world::GROUND_TOP_Y;
use crate::screens::Screen;
use crate::settings::Settings;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRuntimeMode>()
            .add_plugins(ThirdPersonCameraPlugin)
            .add_systems(Startup, spawn_camera)
            .add_systems(PostStartup, attach_initial_gameplay_camera)
            .add_systems(OnEnter(Screen::Gameplay), attach_third_person_camera)
            .add_systems(OnExit(Screen::Gameplay), detach_third_person_camera)
            .add_systems(Update, cycle_camera_mode.run_if(in_state(Screen::Gameplay)))
            .add_systems(
                PostUpdate,
                prevent_camera_ground_clipping
                    .after(CameraSyncSet)
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(Screen::Gameplay)),
            );
    }
}

/// Marks the primary scene camera entity.
#[derive(Component)]
pub struct SceneCamera;

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq, Default)]
enum CameraRuntimeMode {
    Tight,
    #[default]
    Default,
}

fn spawn_camera(mut commands: Commands) {
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
        return;
    };

    insert_gameplay_camera_components(&mut commands, cam, &settings);
}

fn attach_third_person_camera(
    mut commands: Commands,
    settings: Res<Settings>,
    camera: Query<Entity, (With<SceneCamera>, Without<ThirdPersonCamera>)>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };

    insert_gameplay_camera_components(&mut commands, cam, &settings);
}

fn insert_gameplay_camera_components(commands: &mut Commands, cam: Entity, settings: &Settings) {
    commands.entity(cam).insert((
        ThirdPersonCamera {
            zoom: Zoom::new(3.0, 12.0),
            sensitivity: Vec2::new(3.8, 2.8),
            zoom_sensitivity: 0.7,
            mouse_orbit_button_enabled: false,
            cursor_lock_toggle_enabled: false,
            cursor_lock_active: true,
            cursor_lock_key: KeyCode::Tab,
            offset_enabled: true,
            offset: bevy_third_person_camera::Offset::new(0.7, 0.35),
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: settings.fov.to_radians(),
            ..Default::default()
        }),
    ));
}

fn cycle_camera_mode(
    action_state: Res<ActionState>,
    mut mode: ResMut<CameraRuntimeMode>,
    mut camera: Query<&mut ThirdPersonCamera, With<SceneCamera>>,
) {
    if !action_state.just_pressed(GameAction::CycleCameraMode) {
        return;
    }
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };

    *mode = match *mode {
        CameraRuntimeMode::Default => CameraRuntimeMode::Tight,
        CameraRuntimeMode::Tight => CameraRuntimeMode::Default,
    };

    match *mode {
        CameraRuntimeMode::Default => {
            cam.zoom = Zoom::new(3.0, 12.0);
            cam.offset = bevy_third_person_camera::Offset::new(0.7, 0.25);
        }
        CameraRuntimeMode::Tight => {
            cam.zoom = Zoom::new(2.0, 8.0);
            cam.offset = bevy_third_person_camera::Offset::new(0.35, 0.15);
        }
    }
}

fn detach_third_person_camera(
    mut commands: Commands,
    camera: Query<Entity, With<ThirdPersonCamera>>,
) {
    if let Ok(cam) = camera.single() {
        commands.entity(cam).remove::<ThirdPersonCamera>();
    }
}

const CAMERA_GROUND_CLEARANCE: f32 = 0.25;

fn prevent_camera_ground_clipping(
    mut camera: Query<
        &mut Transform,
        (
            With<SceneCamera>,
            With<ThirdPersonCamera>,
            Without<ThirdPersonCameraTarget>,
        ),
    >,
) {
    let Ok(mut camera_tf) = camera.single_mut() else {
        return;
    };

    let min_camera_y = GROUND_TOP_Y + CAMERA_GROUND_CLEARANCE;
    if camera_tf.translation.y < min_camera_y {
        camera_tf.translation.y = min_camera_y;
    }
}
