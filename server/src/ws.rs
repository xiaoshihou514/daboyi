use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt as _;
use shared::{ClientMsg, ServerMsg};

use crate::{game::GameSimulation, AppState};

pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                // Client sends JSON text (small messages).
                Message::Text(text) => {
                    let client_msg = match serde_json::from_str::<ClientMsg>(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Malformed client message: {e}");
                            continue;
                        }
                    };
                    let reply = handle_msg(client_msg, &state);
                    // Server replies with bincode binary for performance.
                    match bincode::serialize(&reply) {
                        Ok(bytes) => {
                            if session.binary(bytes).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => eprintln!("bincode serialize error: {e}"),
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

fn handle_msg(msg: ClientMsg, state: &AppState) -> ServerMsg {
    match msg {
        ClientMsg::Tick => {
            let orders = state.command_queue.lock().unwrap().drain(..).collect();
            let mut gs = state.game_state.lock().unwrap();
            gs.apply_commands(orders);
            gs.advance();
            // Persist every 300 ticks to avoid DB overhead each tick.
            if gs.tick % 300 == 0 {
                state.db.lock().unwrap().save_state(&gs).ok();
            }
            ServerMsg::StateSnapshot(gs.clone())
        }
        ClientMsg::IssueOrder(order) => {
            state.command_queue.lock().unwrap().push(order);
            ServerMsg::Ack
        }
    }
}
