//! Mail server plugin: send / list / mark-read against the persistent mailbox.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::common::mail::*;
use crate::net::ReliableChannel;
use crate::server::db;
use crate::server::online::OnlinePlayers;

pub struct MailServerPlugin;

impl Plugin for MailServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_mail_ops);
    }
}

fn handle_mail_ops(
    online: Res<OnlinePlayers>,
    mut conn_q: Query<
        (
            Entity,
            &mut MessageReceiver<SendMailMessage>,
            &mut MessageReceiver<RequestMailMessage>,
            &mut MessageReceiver<MarkMailReadMessage>,
            &mut MessageSender<MailListMessage>,
            &mut MessageSender<MailActionResultMessage>,
        ),
        With<ClientOf>,
    >,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut send_rx, mut req_rx, mut read_rx, mut list_tx, mut result_tx) in
        conn_q.iter_mut()
    {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in send_rx.receive() {
            let subject: String = msg.subject.trim().chars().take(MAX_MAIL_SUBJECT_LEN).collect();
            let body: String = msg.body.trim().chars().take(MAX_MAIL_BODY_LEN).collect();
            let from_name = online
                .name_of(char_id)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Char#{char_id}"));
            match db::find_character_by_name(&conn, msg.to_name.trim()) {
                Ok(Some(to_id)) => {
                    match db::insert_mail(&conn, to_id, &from_name, &subject, &body) {
                        Ok(_) => result_tx.send::<ReliableChannel>(MailActionResultMessage {
                            ok: true,
                            message: format!("Mail sent to {}", msg.to_name.trim()),
                        }),
                        Err(e) => result_tx.send::<ReliableChannel>(MailActionResultMessage {
                            ok: false,
                            message: format!("DB error: {e}"),
                        }),
                    }
                }
                _ => result_tx.send::<ReliableChannel>(MailActionResultMessage {
                    ok: false,
                    message: "No such character".into(),
                }),
            }
        }

        for _ in req_rx.receive() {
            let entries = db::load_mail(&conn, char_id as i64)
                .unwrap_or_default()
                .into_iter()
                .map(|(id, from_name, subject, body, read)| MailEntry {
                    id: id as u64,
                    from_name,
                    subject,
                    body,
                    read,
                })
                .collect();
            list_tx.send::<ReliableChannel>(MailListMessage { entries });
        }

        for msg in read_rx.receive() {
            let _ = db::mark_mail_read(&conn, msg.mail_id as i64, char_id as i64);
        }
    }
}
