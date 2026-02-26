use bevy::prelude::*;
use futures_util::{SinkExt, StreamExt};
use shared::{ClientMsg, GameState, ServerMsg};
use std::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub struct NetPlugin;

/// Send commands from Bevy systems → WS background task.
/// `UnboundedSender<T: Send>` is Send + Sync, safe as a Bevy resource.
#[derive(Resource)]
pub struct CmdSender(pub tokio::sync::mpsc::UnboundedSender<ClientMsg>);

/// Receive state snapshots from the WS background task → Bevy.
/// Wrapped in Mutex because `std::sync::mpsc::Receiver` is Send but not Sync.
#[derive(Resource)]
pub struct StateReceiver(pub Mutex<mpsc::Receiver<GameState>>);

/// Latest game state received from server; re-set on every tick response.
#[derive(Resource, Default)]
pub struct LatestGameState(pub Option<GameState>);

/// Controls how often a `Tick` message is sent (i.e. the game speed).
/// Modify `0.duration()` at runtime to change speed; set to very long to pause.
#[derive(Resource)]
pub struct GameSpeed(pub Timer);

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMsg>();
        let (state_tx, state_rx) = mpsc::sync_channel::<GameState>(8);

        // Run the WS I/O loop on a dedicated OS thread with its own tokio runtime.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(ws_loop(cmd_rx, state_tx));
        });

        app.insert_resource(CmdSender(cmd_tx))
            .insert_resource(StateReceiver(Mutex::new(state_rx)))
            .insert_resource(LatestGameState::default())
            // Default: one tick per second (1× speed). Adjust to taste.
            .insert_resource(GameSpeed(Timer::from_seconds(1.0, TimerMode::Repeating)))
            .add_systems(Update, (tick_sender, state_receiver));
    }
}

/// Background async task: maintains the WebSocket connection.
async fn ws_loop(
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<ClientMsg>,
    state_tx: mpsc::SyncSender<GameState>,
) {
    let url = "ws://127.0.0.1:8080/ws";

    let (ws_stream, _) = match connect_async(url).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to connect to server at {url}: {e}");
            return;
        }
    };

    let (mut sink, mut stream) = ws_stream.split();

    loop {
        tokio::select! {
            // Incoming message from server.
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ServerMsg>(&text) {
                            Ok(ServerMsg::StateSnapshot(gs)) => { state_tx.send(gs).ok(); }
                            Ok(ServerMsg::Ack) => {}
                            Err(e) => eprintln!("Bad server message: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Outgoing command from a Bevy system.
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(c) => {
                        if let Ok(json) = serde_json::to_string(&c) {
                            sink.send(Message::Text(json)).await.ok();
                        }
                    }
                    None => break,
                }
            }
        }
    }
}

/// Fires a `Tick` at the configured game speed interval.
fn tick_sender(time: Res<Time>, mut speed: ResMut<GameSpeed>, sender: Res<CmdSender>) {
    if speed.0.tick(time.delta()).just_finished() {
        sender.0.send(ClientMsg::Tick).ok();
    }
}

/// Drains all pending state snapshots and keeps the latest one.
fn state_receiver(receiver: Res<StateReceiver>, mut latest: ResMut<LatestGameState>) {
    let rx = receiver.0.lock().unwrap();
    while let Ok(state) = rx.try_recv() {
        latest.0 = Some(state);
    }
}
