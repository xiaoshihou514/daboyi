use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt as _;
use serde::Serialize;
use shared::{ClientMsg, GameState};

use crate::{game::GameSimulation, AppState};

/// Borrowed version of ServerMsg for zero-copy serialization.
/// Variant order MUST match shared::ServerMsg for bincode compatibility.
#[derive(Serialize)]
enum ServerMsgRef<'a> {
    StateSnapshot(&'a GameState),
    Ack,
}

pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                Message::Text(text) => {
                    let client_msg = match serde_json::from_str::<ClientMsg>(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Malformed client message: {e}");
                            continue;
                        }
                    };
                    let bytes = handle_msg(client_msg, &state);
                    if session.binary(bytes).await.is_err() {
                        break;
                    }
                }
                Message::Ping(bytes) => {
                    if session.pong(&bytes).await.is_err() {
                        break;
                    }
                }
                Message::Close(reason) => {
                    session.close(reason).await.ok();
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(response)
}

/// Handle a client message and return pre-serialized bincode bytes.
/// For Tick: serializes directly from the locked GameState reference,
/// avoiding a full clone of ~61K provinces.
fn handle_msg(msg: ClientMsg, state: &AppState) -> Vec<u8> {
    match msg {
        ClientMsg::Tick => {
            let orders = state.command_queue.lock().unwrap().drain(..).collect();
            let player_country = state.player_country.lock().unwrap().clone();
            let mut gs = state.game_state.lock().unwrap();
            gs.apply_commands(orders, player_country.as_deref());
            gs.advance();
            if gs.tick % 300 == 0 {
                state.db.lock().unwrap().save_state(&gs).ok();
            }
            bincode::serialize(&ServerMsgRef::StateSnapshot(&*gs)).unwrap_or_default()
        }
        ClientMsg::FetchState => {
            let gs = state.game_state.lock().unwrap();
            bincode::serialize(&ServerMsgRef::StateSnapshot(&*gs)).unwrap_or_default()
        }
        ClientMsg::IssueOrder(order) => {
            state.command_queue.lock().unwrap().push(order);
            bincode::serialize(&ServerMsgRef::Ack).unwrap_or_default()
        }
        ClientMsg::SetPlayerCountry(tag) => {
            *state.player_country.lock().unwrap() = Some(tag);
            bincode::serialize(&ServerMsgRef::Ack).unwrap_or_default()
        }
    }
}
