use application::ClientCommand;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RoutedCommand {
    pub session_id: Uuid,
    pub command: ClientCommand,
}

#[derive(Debug)]
pub enum WorldMessage {
    RegisterMap {
        map_id: i32,
        tx: mpsc::Sender<RoutedCommand>,
    },
    RouteToMap {
        map_id: i32,
        session_id: Uuid,
        command: ClientCommand,
    },
    Broadcast {
        message: String,
    },
    GetStats {
        reply: oneshot::Sender<WorldStats>,
    },
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WorldStats {
    pub online_players: u64,
    pub players_by_map: Vec<(i32, u64)>,
}

#[derive(Clone)]
pub struct WorldHandle {
    tx: mpsc::Sender<WorldMessage>,
}

impl WorldHandle {
    pub fn new(tx: mpsc::Sender<WorldMessage>) -> Self {
        Self { tx }
    }

    pub async fn route_to_map(&self, map_id: i32, session_id: Uuid, command: ClientCommand) {
        let _ = self
            .tx
            .send(WorldMessage::RouteToMap {
                map_id,
                session_id,
                command,
            })
            .await;
    }

    pub async fn get_stats(&self) -> WorldStats {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(WorldMessage::GetStats { reply: reply_tx })
            .await
            .is_err()
        {
            return WorldStats::default();
        }
        reply_rx.await.unwrap_or_default()
    }

    pub async fn broadcast(&self, message: String) {
        let _ = self.tx.send(WorldMessage::Broadcast { message }).await;
    }
}

pub struct WorldCoordinator {
    rx: mpsc::Receiver<WorldMessage>,
    map_senders: HashMap<i32, mpsc::Sender<RoutedCommand>>,
    players_by_map: HashMap<i32, u64>,
    session_map: HashMap<Uuid, i32>,
}

impl WorldCoordinator {
    pub fn new(rx: mpsc::Receiver<WorldMessage>) -> Self {
        Self {
            rx,
            map_senders: HashMap::new(),
            players_by_map: HashMap::new(),
            session_map: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                WorldMessage::RegisterMap { map_id, tx } => {
                    self.map_senders.insert(map_id, tx);
                    self.players_by_map.entry(map_id).or_insert(0);
                    tracing::info!(map_id, "map registered");
                }
                WorldMessage::RouteToMap {
                    map_id,
                    session_id,
                    command,
                } => {
                    match command {
                        ClientCommand::EnterWorld => {
                            let prev = self.session_map.insert(session_id, map_id);
                            if prev != Some(map_id) {
                                if let Some(prev_map) = prev {
                                    let prev_counter =
                                        self.players_by_map.entry(prev_map).or_insert(0);
                                    *prev_counter = prev_counter.saturating_sub(1);
                                }
                                let counter = self.players_by_map.entry(map_id).or_insert(0);
                                *counter = counter.saturating_add(1);
                            }
                        }
                        ClientCommand::Logout => {
                            let map = self.session_map.remove(&session_id).unwrap_or(map_id);
                            let counter = self.players_by_map.entry(map).or_insert(0);
                            *counter = counter.saturating_sub(1);
                        }
                        _ => {}
                    }
                    if let Some(map_tx) = self.map_senders.get(&map_id) {
                        let _ = map_tx
                            .send(RoutedCommand {
                                session_id,
                                command,
                            })
                            .await;
                    }
                }
                WorldMessage::Broadcast { message } => {
                    tracing::info!(%message, "world broadcast");
                }
                WorldMessage::GetStats { reply } => {
                    let online_players: u64 = self.players_by_map.values().sum();
                    let players_by_map = self
                        .players_by_map
                        .iter()
                        .map(|(map, count)| (*map, *count))
                        .collect();
                    let _ = reply.send(WorldStats {
                        online_players,
                        players_by_map,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn world_stats_changes_with_enter_world_and_logout() {
        let (world_tx, world_rx) = mpsc::channel(16);
        let handle = WorldHandle::new(world_tx.clone());
        let coordinator_task = tokio::spawn(WorldCoordinator::new(world_rx).run());

        let (map_tx, mut map_rx) = mpsc::channel::<RoutedCommand>(16);
        world_tx
            .send(WorldMessage::RegisterMap {
                map_id: 1,
                tx: map_tx,
            })
            .await
            .expect("register map");

        let session_id = Uuid::new_v4();
        handle
            .route_to_map(1, session_id, ClientCommand::EnterWorld)
            .await;
        let _ = map_rx.recv().await.expect("map command");

        let stats = handle.get_stats().await;
        assert_eq!(stats.online_players, 1);
        assert_eq!(
            stats
                .players_by_map
                .iter()
                .find(|(m, _)| *m == 1)
                .map(|(_, c)| *c),
            Some(1)
        );

        handle
            .route_to_map(1, session_id, ClientCommand::Logout)
            .await;
        let _ = map_rx.recv().await.expect("map command");

        let stats = handle.get_stats().await;
        assert_eq!(stats.online_players, 0);
        assert_eq!(
            stats
                .players_by_map
                .iter()
                .find(|(m, _)| *m == 1)
                .map(|(_, c)| *c),
            Some(0)
        );

        coordinator_task.abort();
    }

    #[tokio::test]
    async fn world_stats_do_not_double_count_same_session() {
        let (world_tx, world_rx) = mpsc::channel(16);
        let handle = WorldHandle::new(world_tx.clone());
        let coordinator_task = tokio::spawn(WorldCoordinator::new(world_rx).run());

        let (map_tx, mut map_rx) = mpsc::channel::<RoutedCommand>(16);
        world_tx
            .send(WorldMessage::RegisterMap {
                map_id: 1,
                tx: map_tx,
            })
            .await
            .expect("register map");

        let session_id = Uuid::new_v4();
        handle
            .route_to_map(1, session_id, ClientCommand::EnterWorld)
            .await;
        let _ = map_rx.recv().await.expect("map command");

        handle
            .route_to_map(1, session_id, ClientCommand::EnterWorld)
            .await;
        let _ = map_rx.recv().await.expect("map command");

        let stats = handle.get_stats().await;
        assert_eq!(stats.online_players, 1);

        coordinator_task.abort();
    }
}
