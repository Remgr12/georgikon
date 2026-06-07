//! Authoritative guild system.
//!
//! Headline rule: a character may belong to up to [`MAX_GUILDS_PER_CHAR`]
//! guilds, but a guild flagged `exclusive` forbids its members from being in any
//! other guild. The membership-eligibility logic lives in the pure [`can_join`]
//! function (unit-tested below); the systems wire it to the network + DB.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::{HashMap, HashSet};

use crate::common::guild::*;
use crate::net::ReliableChannel;
use crate::server::db;
use crate::server::online::OnlinePlayers;

// ---------------------------------------------------------------------------
// Eligibility logic (pure, testable)
// ---------------------------------------------------------------------------

/// Decide whether a character already in `current` guilds (each `(guild_id,
/// exclusive)`) may join the target guild.
pub fn can_join(
    current: &[(u64, bool)],
    target_guild_id: u64,
    target_exclusive: bool,
) -> Result<(), String> {
    if current.iter().any(|(g, _)| *g == target_guild_id) {
        return Err("You are already a member of this guild".into());
    }
    if current.len() >= MAX_GUILDS_PER_CHAR {
        return Err("You are already in the maximum number of guilds".into());
    }
    if target_exclusive && !current.is_empty() {
        return Err("This guild is exclusive — leave your other guild(s) first".into());
    }
    if current.iter().any(|(_, ex)| *ex) {
        return Err("You are in an exclusive guild and cannot join another".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct GuildData {
    id: u64,
    name: String,
    leader: u64,
    motd: String,
    exclusive: bool,
    join_policy: JoinPolicy,
    members: HashMap<u64, GuildRank>,
}

#[derive(Resource, Default)]
pub struct GuildRegistry {
    guilds: HashMap<u64, GuildData>,
    /// char_id → display name cache (covers offline members).
    names: HashMap<u64, String>,
    /// char_id → guild_ids they have a pending invite to.
    pending: HashMap<u64, HashSet<u64>>,
}

impl GuildRegistry {
    fn load(&mut self, conn: &rusqlite::Connection) {
        let Ok(rows) = db::load_all_guilds(conn) else { return };
        for r in rows {
            self.guilds.insert(
                r.id as u64,
                GuildData {
                    id: r.id as u64,
                    name: r.name,
                    leader: r.leader_char_id as u64,
                    motd: r.motd,
                    exclusive: r.exclusive,
                    join_policy: JoinPolicy::from_str(&r.join_policy),
                    members: HashMap::new(),
                },
            );
        }
        if let Ok(members) = db::load_guild_members_named(conn) {
            for (gid, cid, rank, name) in members {
                self.names.insert(cid as u64, name);
                if let Some(g) = self.guilds.get_mut(&(gid as u64)) {
                    g.members.insert(cid as u64, GuildRank::from_str(&rank));
                }
            }
        }
    }

    /// Guilds the character belongs to, as `(guild_id, exclusive)`.
    fn membership(&self, char_id: u64) -> Vec<(u64, bool)> {
        self.guilds
            .values()
            .filter(|g| g.members.contains_key(&char_id))
            .map(|g| (g.id, g.exclusive))
            .collect()
    }

    fn rank_of(&self, char_id: u64, guild_id: u64) -> Option<GuildRank> {
        self.guilds.get(&guild_id)?.members.get(&char_id).copied()
    }

    fn name_of(&self, char_id: u64, online: &OnlinePlayers) -> String {
        online
            .name_of(char_id)
            .map(|s| s.to_string())
            .or_else(|| self.names.get(&char_id).cloned())
            .unwrap_or_else(|| format!("Char#{char_id}"))
    }

    /// Build the caller's full guild state.
    fn state_for(&self, char_id: u64, online: &OnlinePlayers) -> GuildStateMessage {
        let mut guilds = Vec::new();
        for g in self.guilds.values() {
            let Some(my_rank) = g.members.get(&char_id).copied() else {
                continue;
            };
            let members = g
                .members
                .iter()
                .map(|(cid, rank)| GuildMemberInfo {
                    char_id: *cid,
                    name: self.name_of(*cid, online),
                    rank: *rank,
                    online: online.is_online(*cid),
                })
                .collect();
            guilds.push(GuildInfo {
                id: g.id,
                name: g.name.clone(),
                motd: g.motd.clone(),
                exclusive: g.exclusive,
                join_policy: g.join_policy,
                members,
                my_rank,
            });
        }
        GuildStateMessage { guilds }
    }

    fn browse(&self) -> GuildListMessage {
        let guilds = self
            .guilds
            .values()
            .map(|g| GuildBrowseEntry {
                id: g.id,
                name: g.name.clone(),
                member_count: g.members.len() as u32,
                join_policy: g.join_policy,
                exclusive: g.exclusive,
            })
            .collect();
        GuildListMessage { guilds }
    }
}

// ---------------------------------------------------------------------------
// Outbox: decouples "who to notify" from the per-connection sender queries.
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct GuildOutbox {
    results: Vec<(u64, GuildActionResultMessage)>,
    invites: Vec<(u64, GuildInvitePushMessage)>,
    lists: Vec<(u64, GuildListMessage)>,
    /// Characters whose full guild state should be re-sent.
    dirty: HashSet<u64>,
}

impl GuildOutbox {
    fn result(&mut self, char_id: u64, ok: bool, msg: impl Into<String>) {
        self.results.push((
            char_id,
            GuildActionResultMessage {
                ok,
                message: msg.into(),
            },
        ));
    }
}

/// Mark every member of a guild dirty (so their panels refresh).
fn mark_guild_dirty(reg: &GuildRegistry, guild_id: u64, outbox: &mut GuildOutbox) {
    if let Some(g) = reg.guilds.get(&guild_id) {
        for cid in g.members.keys() {
            outbox.dirty.insert(*cid);
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct GuildServerPlugin;

impl Plugin for GuildServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuildRegistry>();
        app.init_resource::<GuildOutbox>();
        app.add_systems(Startup, load_registry);
        app.add_systems(
            Update,
            (
                handle_create_disband_list,
                handle_invites,
                handle_membership,
                handle_settings,
                flush_outbox,
            )
                .chain(),
        );
    }
}

fn load_registry(mut reg: ResMut<GuildRegistry>) {
    if let Ok(conn) = db::open().and_then(|c| db::init(&c).map(|_| c)) {
        reg.load(&conn);
        tracing::info!("Loaded {} guild(s) from DB", reg.guilds.len());
    }
}

// ---------------------------------------------------------------------------
// Create / disband / browse
// ---------------------------------------------------------------------------

fn handle_create_disband_list(
    mut reg: ResMut<GuildRegistry>,
    mut outbox: ResMut<GuildOutbox>,
    online: Res<OnlinePlayers>,
    mut q: Query<
        (
            Entity,
            &mut MessageReceiver<CreateGuildMessage>,
            &mut MessageReceiver<DisbandGuildMessage>,
            &mut MessageReceiver<RequestGuildListMessage>,
        ),
        With<ClientOf>,
    >,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut create_rx, mut disband_rx, mut list_rx) in q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in create_rx.receive() {
            let name = msg.name.trim().to_string();
            if name.is_empty() || name.len() > MAX_GUILD_NAME_LEN {
                outbox.result(char_id, false, "Invalid guild name");
                continue;
            }
            if reg.guilds.values().any(|g| g.name.eq_ignore_ascii_case(&name)) {
                outbox.result(char_id, false, "A guild with that name exists");
                continue;
            }
            // Creating a guild counts as joining it — apply eligibility (a new
            // guild is non-exclusive, so this only enforces the cap / exclusivity).
            let current = reg.membership(char_id);
            if let Err(e) = can_join(&current, u64::MAX, false) {
                outbox.result(char_id, false, e);
                continue;
            }
            match db::create_guild(&conn, &name, char_id as i64) {
                Ok(id) => {
                    let id = id as u64;
                    let mut members = HashMap::new();
                    members.insert(char_id, GuildRank::Leader);
                    let display_name = reg.name_of(char_id, &online);
                    reg.guilds.insert(
                        id,
                        GuildData {
                            id,
                            name: name.clone(),
                            leader: char_id,
                            motd: String::new(),
                            exclusive: false,
                            join_policy: JoinPolicy::InviteOnly,
                            members,
                        },
                    );
                    reg.names.insert(char_id, display_name);
                    outbox.result(char_id, true, format!("Founded guild '{name}'"));
                    outbox.dirty.insert(char_id);
                }
                Err(e) => outbox.result(char_id, false, format!("DB error: {e}")),
            }
        }

        for msg in disband_rx.receive() {
            let gid = msg.guild_id;
            match reg.rank_of(char_id, gid) {
                Some(GuildRank::Leader) => {
                    mark_guild_dirty(&reg, gid, &mut outbox);
                    let _ = db::delete_guild(&conn, gid as i64);
                    reg.guilds.remove(&gid);
                    outbox.result(char_id, true, "Guild disbanded");
                }
                _ => outbox.result(char_id, false, "Only the leader can disband"),
            }
        }

        for _ in list_rx.receive() {
            outbox.lists.push((char_id, reg.browse()));
        }
    }
}

// ---------------------------------------------------------------------------
// Invites (request + response)
// ---------------------------------------------------------------------------

fn handle_invites(
    mut reg: ResMut<GuildRegistry>,
    mut outbox: ResMut<GuildOutbox>,
    online: Res<OnlinePlayers>,
    mut q: Query<
        (
            Entity,
            &mut MessageReceiver<GuildInviteRequestMessage>,
            &mut MessageReceiver<GuildInviteResponseMessage>,
        ),
        With<ClientOf>,
    >,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut req_rx, mut resp_rx) in q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in req_rx.receive() {
            let gid = msg.guild_id;
            // Inviter must have management rights.
            match reg.rank_of(char_id, gid) {
                Some(r) if r.can_manage_members() => {}
                _ => {
                    outbox.result(char_id, false, "You can't invite to this guild");
                    continue;
                }
            }
            let Some(target) = online.char_by_name(&msg.target_name) else {
                outbox.result(char_id, false, "That player is not online");
                continue;
            };
            // Pre-check eligibility so we don't dangle a useless invite.
            let target_membership = reg.membership(target);
            let target_exclusive = reg.guilds.get(&gid).map(|g| g.exclusive).unwrap_or(false);
            if let Err(e) = can_join(&target_membership, gid, target_exclusive) {
                outbox.result(char_id, false, format!("Can't invite: {e}"));
                continue;
            }
            reg.pending.entry(target).or_default().insert(gid);
            let guild_name = reg.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
            outbox.invites.push((
                target,
                GuildInvitePushMessage {
                    guild_id: gid,
                    guild_name,
                    from_name: reg.name_of(char_id, &online),
                },
            ));
            outbox.result(char_id, true, "Invite sent");
        }

        for msg in resp_rx.receive() {
            let gid = msg.guild_id;
            let had_invite = reg
                .pending
                .get(&char_id)
                .map(|s| s.contains(&gid))
                .unwrap_or(false);
            if !had_invite {
                outbox.result(char_id, false, "No pending invite for that guild");
                continue;
            }
            reg.pending.entry(char_id).or_default().remove(&gid);
            if !msg.accept {
                outbox.result(char_id, true, "Invite declined");
                continue;
            }
            // Re-validate eligibility at accept time.
            let current = reg.membership(char_id);
            let exclusive = reg.guilds.get(&gid).map(|g| g.exclusive).unwrap_or(false);
            if let Err(e) = can_join(&current, gid, exclusive) {
                outbox.result(char_id, false, e);
                continue;
            }
            let name = reg.name_of(char_id, &online);
            if let Some(g) = reg.guilds.get_mut(&gid) {
                g.members.insert(char_id, GuildRank::Member);
            }
            reg.names.insert(char_id, name);
            let _ = db::insert_guild_member(&conn, gid as i64, char_id as i64, "Member");
            mark_guild_dirty(&reg, gid, &mut outbox);
            outbox.result(char_id, true, "Joined guild");
        }
    }
}

// ---------------------------------------------------------------------------
// Membership ops: leave / kick / set rank
// ---------------------------------------------------------------------------

fn handle_membership(
    mut reg: ResMut<GuildRegistry>,
    mut outbox: ResMut<GuildOutbox>,
    online: Res<OnlinePlayers>,
    mut q: Query<
        (
            Entity,
            &mut MessageReceiver<LeaveGuildMessage>,
            &mut MessageReceiver<KickGuildMemberMessage>,
            &mut MessageReceiver<SetGuildRankMessage>,
        ),
        With<ClientOf>,
    >,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut leave_rx, mut kick_rx, mut rank_rx) in q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in leave_rx.receive() {
            let gid = msg.guild_id;
            if reg.rank_of(char_id, gid) == Some(GuildRank::Leader) {
                outbox.result(char_id, false, "Leaders must disband or promote first");
                continue;
            }
            if reg.guilds.get_mut(&gid).map(|g| g.members.remove(&char_id)).flatten().is_some() {
                let _ = db::delete_guild_member(&conn, gid as i64, char_id as i64);
                mark_guild_dirty(&reg, gid, &mut outbox);
                outbox.dirty.insert(char_id);
                outbox.result(char_id, true, "Left guild");
            } else {
                outbox.result(char_id, false, "You are not in that guild");
            }
        }

        for msg in kick_rx.receive() {
            let gid = msg.guild_id;
            let target = msg.target_char_id;
            match reg.rank_of(char_id, gid) {
                Some(r) if r.can_manage_members() => {}
                _ => {
                    outbox.result(char_id, false, "You can't kick from this guild");
                    continue;
                }
            }
            if target == char_id || reg.rank_of(target, gid) == Some(GuildRank::Leader) {
                outbox.result(char_id, false, "Can't kick that member");
                continue;
            }
            if reg.guilds.get_mut(&gid).map(|g| g.members.remove(&target)).flatten().is_some() {
                let _ = db::delete_guild_member(&conn, gid as i64, target as i64);
                mark_guild_dirty(&reg, gid, &mut outbox);
                outbox.dirty.insert(target);
                outbox.result(char_id, true, "Member kicked");
            } else {
                outbox.result(char_id, false, "No such member");
            }
        }

        for msg in rank_rx.receive() {
            let gid = msg.guild_id;
            if reg.rank_of(char_id, gid) != Some(GuildRank::Leader) {
                outbox.result(char_id, false, "Only the leader can set ranks");
                continue;
            }
            let target = msg.target_char_id;
            if reg.rank_of(target, gid).is_none() {
                outbox.result(char_id, false, "No such member");
                continue;
            }
            // Promoting someone to Leader transfers leadership.
            if msg.rank == GuildRank::Leader {
                if let Some(g) = reg.guilds.get_mut(&gid) {
                    g.members.insert(char_id, GuildRank::Officer);
                    g.members.insert(target, GuildRank::Leader);
                    g.leader = target;
                }
                let _ = db::update_guild_member_rank(&conn, gid as i64, char_id as i64, "Officer");
                let _ = db::update_guild_member_rank(&conn, gid as i64, target as i64, "Leader");
            } else {
                if let Some(g) = reg.guilds.get_mut(&gid) {
                    g.members.insert(target, msg.rank);
                }
                let _ = db::update_guild_member_rank(
                    &conn,
                    gid as i64,
                    target as i64,
                    msg.rank.as_str(),
                );
            }
            mark_guild_dirty(&reg, gid, &mut outbox);
            outbox.result(char_id, true, "Rank updated");
        }
    }
}

// ---------------------------------------------------------------------------
// Settings: exclusivity / join policy / motd
// ---------------------------------------------------------------------------

fn handle_settings(
    mut reg: ResMut<GuildRegistry>,
    mut outbox: ResMut<GuildOutbox>,
    online: Res<OnlinePlayers>,
    mut q: Query<
        (
            Entity,
            &mut MessageReceiver<SetGuildSettingsMessage>,
            &mut MessageReceiver<SetGuildMotdMessage>,
        ),
        With<ClientOf>,
    >,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut settings_rx, mut motd_rx) in q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in settings_rx.receive() {
            let gid = msg.guild_id;
            if reg.rank_of(char_id, gid) != Some(GuildRank::Leader) {
                outbox.result(char_id, false, "Only the leader can change settings");
                continue;
            }
            // Rule 4: you may only flip a guild to exclusive if no current member
            // belongs to any *other* guild.
            if msg.exclusive {
                let member_ids: Vec<u64> = reg
                    .guilds
                    .get(&gid)
                    .map(|g| g.members.keys().copied().collect())
                    .unwrap_or_default();
                let blocker = member_ids.iter().find(|cid| {
                    reg.membership(**cid).iter().any(|(other, _)| *other != gid)
                });
                if let Some(cid) = blocker {
                    let name = reg.name_of(*cid, &online);
                    outbox.result(
                        char_id,
                        false,
                        format!("Can't make exclusive: {name} is in another guild"),
                    );
                    continue;
                }
            }
            if let Some(g) = reg.guilds.get_mut(&gid) {
                g.exclusive = msg.exclusive;
                g.join_policy = msg.join_policy;
            }
            let _ = db::update_guild_settings(
                &conn,
                gid as i64,
                msg.exclusive,
                msg.join_policy.as_str(),
            );
            mark_guild_dirty(&reg, gid, &mut outbox);
            outbox.result(char_id, true, "Settings updated");
        }

        for msg in motd_rx.receive() {
            let gid = msg.guild_id;
            match reg.rank_of(char_id, gid) {
                Some(r) if r.can_manage_members() => {}
                _ => {
                    outbox.result(char_id, false, "You can't edit the MOTD");
                    continue;
                }
            }
            let motd = msg.motd.chars().take(200).collect::<String>();
            if let Some(g) = reg.guilds.get_mut(&gid) {
                g.motd = motd.clone();
            }
            let _ = db::update_guild_motd(&conn, gid as i64, &motd);
            mark_guild_dirty(&reg, gid, &mut outbox);
            outbox.result(char_id, true, "MOTD updated");
        }
    }
}

// ---------------------------------------------------------------------------
// Flush: deliver queued messages to the right connections.
// ---------------------------------------------------------------------------

fn flush_outbox(
    mut outbox: ResMut<GuildOutbox>,
    reg: Res<GuildRegistry>,
    online: Res<OnlinePlayers>,
    mut result_tx: Query<&mut MessageSender<GuildActionResultMessage>, With<ClientOf>>,
    mut invite_tx: Query<&mut MessageSender<GuildInvitePushMessage>, With<ClientOf>>,
    mut list_tx: Query<&mut MessageSender<GuildListMessage>, With<ClientOf>>,
    mut state_tx: Query<&mut MessageSender<GuildStateMessage>, With<ClientOf>>,
) {
    let results = std::mem::take(&mut outbox.results);
    for (char_id, msg) in results {
        if let Some(conn) = online.conn_of(char_id) {
            if let Ok(mut tx) = result_tx.get_mut(conn) {
                tx.send::<ReliableChannel>(msg);
            }
        }
    }

    let invites = std::mem::take(&mut outbox.invites);
    for (char_id, msg) in invites {
        if let Some(conn) = online.conn_of(char_id) {
            if let Ok(mut tx) = invite_tx.get_mut(conn) {
                tx.send::<ReliableChannel>(msg);
            }
        }
    }

    let lists = std::mem::take(&mut outbox.lists);
    for (char_id, msg) in lists {
        if let Some(conn) = online.conn_of(char_id) {
            if let Ok(mut tx) = list_tx.get_mut(conn) {
                tx.send::<ReliableChannel>(msg);
            }
        }
    }

    let dirty = std::mem::take(&mut outbox.dirty);
    for char_id in dirty {
        if let Some(conn) = online.conn_of(char_id) {
            if let Ok(mut tx) = state_tx.get_mut(conn) {
                tx.send::<ReliableChannel>(reg.state_for(char_id, &online));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public read access for other server systems (chat routing, islands).
// ---------------------------------------------------------------------------

impl GuildRegistry {
    /// Online members of a guild (for guild chat routing).
    pub fn member_ids(&self, guild_id: u64) -> Vec<u64> {
        self.guilds
            .get(&guild_id)
            .map(|g| g.members.keys().copied().collect())
            .unwrap_or_default()
    }

    /// The guild ids a character belongs to (for chat `/g1`, `/g2`).
    pub fn guild_ids_of(&self, char_id: u64) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .guilds
            .values()
            .filter(|g| g.members.contains_key(&char_id))
            .map(|g| g.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Whether the character may build on the given guild's island.
    pub fn can_build(&self, char_id: u64, guild_id: u64) -> bool {
        self.rank_of(char_id, guild_id)
            .map(|r| r.can_build())
            .unwrap_or(false)
    }

    pub fn is_member(&self, char_id: u64, guild_id: u64) -> bool {
        self.rank_of(char_id, guild_id).is_some()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_when_empty_ok() {
        assert!(can_join(&[], 1, false).is_ok());
        assert!(can_join(&[], 1, true).is_ok());
    }

    #[test]
    fn second_non_exclusive_ok() {
        assert!(can_join(&[(1, false)], 2, false).is_ok());
    }

    #[test]
    fn third_guild_rejected() {
        assert!(can_join(&[(1, false), (2, false)], 3, false).is_err());
    }

    #[test]
    fn target_exclusive_requires_empty() {
        assert!(can_join(&[(1, false)], 2, true).is_err());
        assert!(can_join(&[], 2, true).is_ok());
    }

    #[test]
    fn in_exclusive_blocks_second() {
        assert!(can_join(&[(1, true)], 2, false).is_err());
    }

    #[test]
    fn duplicate_join_rejected() {
        assert!(can_join(&[(1, false)], 1, false).is_err());
    }
}
