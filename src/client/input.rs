use bevy::ecs::message::MessageReader;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GameAction {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Jump,
    Sprint,
    Primary,
    Secondary,
    Block,
    Roll,
    TogglePause,
    ChatOpenSend,
    ChatCancel,
    ChatBackspace,
    ChatHistoryPrev,
    ChatHistoryNext,
    CycleCameraMode,
    ZoomIn,
    ZoomOut,
    InventorySort,
    HotbarSlot1,
    HotbarSlot2,
    HotbarSlot3,
    HotbarSlot4,
    HotbarSlot5,
    HotbarSlot6,
}

#[derive(Resource)]
pub struct ActionMap {
    key_to_action: HashMap<KeyCode, Vec<GameAction>>,
}

impl ActionMap {
    fn default_bindings() -> HashMap<KeyCode, Vec<GameAction>> {
        use GameAction::*;
        HashMap::from([
            (KeyCode::KeyW, vec![MoveForward]),
            (KeyCode::KeyS, vec![MoveBack]),
            (KeyCode::KeyA, vec![MoveLeft]),
            (KeyCode::KeyD, vec![MoveRight]),
            (KeyCode::Space, vec![Jump]),
            (KeyCode::ShiftLeft, vec![Sprint]),
            (KeyCode::ControlLeft, vec![Block]),
            (KeyCode::KeyQ, vec![Roll]),
            (KeyCode::KeyZ, vec![Primary]),
            (KeyCode::KeyX, vec![Secondary]),
            (KeyCode::Escape, vec![TogglePause, ChatCancel]),
            (KeyCode::Enter, vec![ChatOpenSend]),
            (KeyCode::Backspace, vec![ChatBackspace]),
            (KeyCode::ArrowUp, vec![ChatHistoryPrev]),
            (KeyCode::ArrowDown, vec![ChatHistoryNext]),
            (KeyCode::KeyV, vec![CycleCameraMode]),
            (KeyCode::KeyR, vec![InventorySort]),
            (KeyCode::KeyC, vec![ZoomIn]),
            (KeyCode::KeyF, vec![ZoomOut]),
            (KeyCode::Digit1, vec![HotbarSlot1]),
            (KeyCode::Digit2, vec![HotbarSlot2]),
            (KeyCode::Digit3, vec![HotbarSlot3]),
            (KeyCode::Digit4, vec![HotbarSlot4]),
            (KeyCode::Digit5, vec![HotbarSlot5]),
            (KeyCode::Digit6, vec![HotbarSlot6]),
        ])
    }
}

impl Default for ActionMap {
    fn default() -> Self {
        Self {
            key_to_action: Self::default_bindings(),
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct ActionState {
    pressed: HashSet<GameAction>,
    just_pressed: HashSet<GameAction>,
    just_released: HashSet<GameAction>,
    pub mouse_delta: Vec2,
}

impl ActionState {
    pub fn pressed(&self, action: GameAction) -> bool {
        self.pressed.contains(&action)
    }

    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.just_pressed.contains(&action)
    }

    pub fn just_released(&self, action: GameAction) -> bool {
        self.just_released.contains(&action)
    }

    pub fn movement_axis(&self) -> Vec2 {
        use GameAction::*;
        let x = (self.pressed(MoveRight) as i8 - self.pressed(MoveLeft) as i8) as f32;
        let y = (self.pressed(MoveForward) as i8 - self.pressed(MoveBack) as i8) as f32;
        Vec2::new(x, y).clamp_length_max(1.0)
    }
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionMap>()
            .init_resource::<ActionState>()
            .add_systems(PreUpdate, update_action_state)
            .add_systems(PreUpdate, update_mouse_delta);
    }
}

fn update_mouse_delta(
    mut mouse_motion: MessageReader<MouseMotion>,
    mut state: ResMut<ActionState>,
) {
    state.mouse_delta = Vec2::ZERO;
    for motion in mouse_motion.read() {
        state.mouse_delta += motion.delta;
    }
}

fn update_action_state(
    keys: Res<ButtonInput<KeyCode>>,
    map: Res<ActionMap>,
    mut state: ResMut<ActionState>,
) {
    state.just_pressed.clear();
    state.just_released.clear();

    for (&key, actions) in &map.key_to_action {
        for &action in actions {
            if keys.just_pressed(key) {
                state.just_pressed.insert(action);
                state.pressed.insert(action);
            }
            if keys.just_released(key) {
                state.just_released.insert(action);
                state.pressed.remove(&action);
            }
            if keys.pressed(key) {
                state.pressed.insert(action);
            }
        }
    }
}
