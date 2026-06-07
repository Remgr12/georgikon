//! Quest tracking: accept/abandon/turn-in, kill + collect objective progress,
//! and the quest log sent to clients. Progress is persisted per character.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::{HashMap, HashSet};

use crate::common::inventory::Inventory;
use crate::common::mob::UnitKind;
use crate::common::quest::*;
use crate::net::ReliableChannel;
use crate::server::db::{self, QuestObjectiveRow, QuestRow};
use crate::server::mobs::MobKilled;
use crate::server::online::OnlinePlayers;
use crate::server::progression::AwardXp;

#[derive(Resource, Default)]
struct QuestDefs {
    quests: Vec<QuestRow>,
    objectives: HashMap<u32, Vec<QuestObjectiveRow>>,
}

#[derive(Clone)]
struct QuestRun {
    state: QuestState,
    counts: Vec<u32>,
}

#[derive(Resource, Default)]
struct QuestProgress {
    by_char: HashMap<u64, HashMap<u32, QuestRun>>,
    loaded: HashSet<u64>,
}

#[derive(Resource, Default)]
struct QuestOutbox {
    dirty: HashSet<u64>,
}

pub struct QuestServerPlugin;

impl Plugin for QuestServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuestDefs>();
        app.init_resource::<QuestProgress>();
        app.init_resource::<QuestOutbox>();
        app.add_systems(Startup, load_defs);
        app.add_systems(Update, (handle_quest_ops, flush_quest_log).chain());
        app.add_observer(on_mob_killed);
    }
}

fn load_defs(mut defs: ResMut<QuestDefs>) {
    let Ok(conn) = db::open() else { return };
    defs.quests = db::load_quests(&conn).unwrap_or_default();
    for o in db::load_quest_objectives(&conn).unwrap_or_default() {
        defs.objectives.entry(o.quest_id).or_default().push(o);
    }
}

/// kill-objective target id → mob kind.
fn target_kind(target_id: u32) -> Option<UnitKind> {
    match target_id {
        1 => Some(UnitKind::Wolf),
        2 => Some(UnitKind::Boar),
        _ => None,
    }
}

fn ensure_loaded(progress: &mut QuestProgress, defs: &QuestDefs, char_id: u64) {
    if progress.loaded.contains(&char_id) {
        return;
    }
    progress.loaded.insert(char_id);
    let map = progress.by_char.entry(char_id).or_default();
    let Ok(conn) = db::open() else { return };
    let rows = db::load_quest_progress(&conn, char_id as i64).unwrap_or_default();
    for (quest_id, obj_idx, count, state) in rows {
        let n_obj = defs.objectives.get(&quest_id).map(|v| v.len()).unwrap_or(0);
        let run = map.entry(quest_id).or_insert_with(|| QuestRun {
            state: parse_state(&state),
            counts: vec![0; n_obj],
        });
        run.state = parse_state(&state);
        if (obj_idx as usize) < run.counts.len() {
            run.counts[obj_idx as usize] = count;
        }
    }
}

fn parse_state(s: &str) -> QuestState {
    match s {
        "complete" => QuestState::Complete,
        "turnedin" => QuestState::TurnedIn,
        _ => QuestState::Active,
    }
}

fn state_str(s: QuestState) -> &'static str {
    match s {
        QuestState::Complete => "complete",
        QuestState::TurnedIn => "turnedin",
        _ => "active",
    }
}

fn persist_run(char_id: u64, quest_id: u32, run: &QuestRun) {
    let Ok(conn) = db::open() else { return };
    for (idx, count) in run.counts.iter().enumerate() {
        let _ = db::upsert_quest_progress(
            &conn,
            char_id as i64,
            quest_id,
            idx as u32,
            *count,
            state_str(run.state),
        );
    }
}

/// Recompute collect-objective counts from the player's inventory and update
/// the run's completion state.
fn refresh_completion(run: &mut QuestRun, objs: &[QuestObjectiveRow], inv: Option<&Inventory>) {
    for (idx, obj) in objs.iter().enumerate() {
        if obj.kind == "collect" {
            if let Some(inv) = inv {
                if idx < run.counts.len() {
                    run.counts[idx] = inv.total_quantity(obj.target_id);
                }
            }
        }
    }
    if run.state == QuestState::Active || run.state == QuestState::Complete {
        let done = objs
            .iter()
            .enumerate()
            .all(|(i, o)| run.counts.get(i).copied().unwrap_or(0) >= o.required);
        run.state = if done {
            QuestState::Complete
        } else {
            QuestState::Active
        };
    }
}

fn handle_quest_ops(
    defs: Res<QuestDefs>,
    mut progress: ResMut<QuestProgress>,
    mut outbox: ResMut<QuestOutbox>,
    online: Res<OnlinePlayers>,
    mut commands: Commands,
    mut inv_q: Query<&mut Inventory>,
    mut conn_q: Query<
        (
            Entity,
            &mut MessageReceiver<AcceptQuestMessage>,
            &mut MessageReceiver<AbandonQuestMessage>,
            &mut MessageReceiver<TurnInQuestMessage>,
        ),
        With<ClientOf>,
    >,
) {
    for (conn, mut accept_rx, mut abandon_rx, mut turnin_rx) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn) else {
            continue;
        };
        ensure_loaded(&mut progress, &defs, char_id);

        for msg in accept_rx.receive() {
            let qid = msg.quest_id;
            let Some(objs) = defs.objectives.get(&qid) else {
                continue;
            };
            let map = progress.by_char.entry(char_id).or_default();
            let already = map
                .get(&qid)
                .map(|r| r.state != QuestState::TurnedIn)
                .unwrap_or(false);
            if already {
                continue;
            }
            let run = QuestRun {
                state: QuestState::Active,
                counts: vec![0; objs.len()],
            };
            persist_run(char_id, qid, &run);
            map.insert(qid, run);
            outbox.dirty.insert(char_id);
        }

        for msg in abandon_rx.receive() {
            if let Some(map) = progress.by_char.get_mut(&char_id) {
                map.remove(&msg.quest_id);
            }
            if let Ok(conn_db) = db::open() {
                let _ = db::delete_quest_progress(&conn_db, char_id as i64, msg.quest_id);
            }
            outbox.dirty.insert(char_id);
        }

        for msg in turnin_rx.receive() {
            let qid = msg.quest_id;
            let Some(objs) = defs.objectives.get(&qid) else {
                continue;
            };
            let Some(quest) = defs.quests.iter().find(|q| q.id == qid) else {
                continue;
            };
            let game = online.game_of(char_id);
            // Refresh from inventory + verify complete.
            let mut inv_ref = game.and_then(|g| inv_q.get(g).ok());
            let complete = {
                let map = progress.by_char.entry(char_id).or_default();
                let Some(run) = map.get_mut(&qid) else {
                    continue;
                };
                refresh_completion(run, objs, inv_ref.take());
                run.state == QuestState::Complete
            };
            if !complete {
                continue;
            }
            // Consume collect items, grant rewards.
            if let Some(g) = game {
                if let Ok(mut inv) = inv_q.get_mut(g) {
                    for obj in objs.iter().filter(|o| o.kind == "collect") {
                        inv.remove_by_item_id(obj.target_id, obj.required);
                    }
                    if quest.reward_item != 0 && quest.reward_qty > 0 {
                        inv.add(quest.reward_item, quest.reward_qty);
                    }
                }
            }
            if quest.reward_xp > 0 {
                commands.trigger(AwardXp {
                    char_id,
                    amount: quest.reward_xp,
                });
            }
            if let Some(run) = progress.by_char.entry(char_id).or_default().get_mut(&qid) {
                run.state = QuestState::TurnedIn;
                persist_run(char_id, qid, run);
            }
            outbox.dirty.insert(char_id);
        }
    }
}

fn on_mob_killed(
    trigger: On<MobKilled>,
    defs: Res<QuestDefs>,
    mut progress: ResMut<QuestProgress>,
    mut outbox: ResMut<QuestOutbox>,
) {
    let ev = trigger.event();
    ensure_loaded(&mut progress, &defs, ev.killer_char);
    let Some(map) = progress.by_char.get_mut(&ev.killer_char) else {
        return;
    };
    let mut changed = false;
    for (qid, run) in map.iter_mut() {
        if run.state != QuestState::Active {
            continue;
        }
        let Some(objs) = defs.objectives.get(qid) else {
            continue;
        };
        for (idx, obj) in objs.iter().enumerate() {
            if obj.kind == "kill" && target_kind(obj.target_id) == Some(ev.kind) {
                if idx < run.counts.len() && run.counts[idx] < obj.required {
                    run.counts[idx] += 1;
                    changed = true;
                }
            }
        }
        if changed {
            let done = objs
                .iter()
                .enumerate()
                .all(|(i, o)| run.counts.get(i).copied().unwrap_or(0) >= o.required);
            if done {
                run.state = QuestState::Complete;
            }
            persist_run(ev.killer_char, *qid, run);
        }
    }
    if changed {
        outbox.dirty.insert(ev.killer_char);
    }
}

fn flush_quest_log(
    defs: Res<QuestDefs>,
    mut progress: ResMut<QuestProgress>,
    mut outbox: ResMut<QuestOutbox>,
    online: Res<OnlinePlayers>,
    inv_q: Query<&Inventory>,
    mut tx_q: Query<&mut MessageSender<QuestLogMessage>, With<ClientOf>>,
) {
    let dirty = std::mem::take(&mut outbox.dirty);
    for char_id in dirty {
        let Some(conn) = online.conn_of(char_id) else {
            continue;
        };
        let inv = online.game_of(char_id).and_then(|g| inv_q.get(g).ok());
        let log = build_log(char_id, &defs, &mut progress, inv);
        if let Ok(mut tx) = tx_q.get_mut(conn) {
            tx.send::<ReliableChannel>(log);
        }
    }
}

fn build_log(
    char_id: u64,
    defs: &QuestDefs,
    progress: &mut QuestProgress,
    inv: Option<&Inventory>,
) -> QuestLogMessage {
    let map = progress.by_char.entry(char_id).or_default();
    let mut quests = Vec::new();
    for quest in &defs.quests {
        let objs = defs.objectives.get(&quest.id).cloned().unwrap_or_default();
        let (state, counts) = match map.get_mut(&quest.id) {
            Some(run) => {
                refresh_completion(run, &objs, inv);
                (run.state, run.counts.clone())
            }
            None => (QuestState::Available, vec![0; objs.len()]),
        };
        let objectives = objs
            .iter()
            .enumerate()
            .map(|(i, o)| QuestObjectiveInfo {
                text: o.text.clone(),
                count: counts.get(i).copied().unwrap_or(0),
                required: o.required,
            })
            .collect();
        quests.push(QuestInfo {
            quest_id: quest.id,
            name: quest.name.clone(),
            description: quest.description.clone(),
            objectives,
            state,
        });
    }
    QuestLogMessage { quests }
}
