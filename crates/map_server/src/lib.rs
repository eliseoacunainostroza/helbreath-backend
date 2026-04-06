use application::ClientCommand;
use observability::{MetricsRegistry, METRICS};
use std::collections::{BTreeMap, HashMap, VecDeque};
use tokio::sync::mpsc;
use tokio::time::{self, Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MapConfig {
    pub map_id: i32,
    pub tick_ms: u64,
    pub command_budget: usize,
}

#[derive(Debug, Clone)]
pub struct IncomingCommand {
    pub session_id: Uuid,
    pub command: ClientCommand,
}

#[derive(Debug, Clone)]
pub enum OutboundEvent {
    Text {
        session_id: Uuid,
        text: String,
    },
    Position {
        session_id: Uuid,
        x: i32,
        y: i32,
    },
    CombatResult {
        attacker: Uuid,
        defender: Uuid,
        damage: i32,
    },
}

#[derive(Debug, Clone)]
pub enum PersistOp {
    SaveCharacterPosition {
        character_id: Uuid,
        map_id: i32,
        x: i32,
        y: i32,
    },
    SaveInventoryChange {
        character_id: Uuid,
        map_id: i32,
        reason: String,
    },
    SaveCombatLog {
        attacker: Uuid,
        defender: Uuid,
        map_id: i32,
        damage: i32,
    },
}

#[derive(Debug, Clone)]
struct PlayerState {
    character_id: Uuid,
    x: i32,
    y: i32,
    hp: i32,
    mp: i32,
}

#[derive(Debug, Clone)]
struct ScheduledEvent {
    run_at_tick: u64,
    event: OutboundEvent,
}

pub struct MapInstance {
    pub config: MapConfig,
    pub rx: mpsc::Receiver<IncomingCommand>,
    pub outbound_tx: mpsc::Sender<OutboundEvent>,
    pub persist_tx: mpsc::Sender<PersistOp>,

    tick_index: u64,
    players: HashMap<Uuid, PlayerState>,
    pending_commands: VecDeque<IncomingCommand>,
    scheduled: BTreeMap<u64, Vec<ScheduledEvent>>,
    pending_attacks: Vec<(Uuid, Uuid)>,
    pending_skill_casts: Vec<(Uuid, i32, Option<Uuid>)>,
    pending_inventory_changes: Vec<(Uuid, String)>,
}

impl MapInstance {
    pub fn new(
        config: MapConfig,
        rx: mpsc::Receiver<IncomingCommand>,
        outbound_tx: mpsc::Sender<OutboundEvent>,
        persist_tx: mpsc::Sender<PersistOp>,
    ) -> Self {
        Self {
            config,
            rx,
            outbound_tx,
            persist_tx,
            tick_index: 0,
            players: HashMap::new(),
            pending_commands: VecDeque::new(),
            scheduled: BTreeMap::new(),
            pending_attacks: Vec::new(),
            pending_skill_casts: Vec::new(),
            pending_inventory_changes: Vec::new(),
        }
    }

    pub async fn run(mut self) {
        let mut ticker = time::interval(Duration::from_millis(self.config.tick_ms));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            let started = Instant::now();
            self.tick_index = self.tick_index.wrapping_add(1);

            self.drain_incoming_queue();
            self.process_tick().await;

            let elapsed = started.elapsed().as_millis() as u64;
            MetricsRegistry::set(&METRICS.tick_duration_ms_last, elapsed);
            MetricsRegistry::set(
                &METRICS.command_queue_depth,
                self.pending_commands.len() as u64,
            );

            if elapsed > self.config.tick_ms {
                MetricsRegistry::inc(&METRICS.tick_overruns_total);
                tracing::warn!(
                    map_id = self.config.map_id,
                    tick = self.tick_index,
                    elapsed_ms = elapsed,
                    budget_ms = self.config.tick_ms,
                    "tick overrun"
                );
            }
        }
    }

    fn drain_incoming_queue(&mut self) {
        let mut accepted = 0usize;
        while accepted < self.config.command_budget {
            match self.rx.try_recv() {
                Ok(cmd) => {
                    self.pending_commands.push_back(cmd);
                    accepted += 1;
                }
                Err(_) => break,
            }
        }
    }

    async fn process_tick(&mut self) {
        self.process_scheduled_events().await;

        let mut commands = Vec::new();
        while let Some(command) = self.pending_commands.pop_front() {
            commands.push(command);
        }

        // Deterministic processing order:
        // 1) input validation + normalize
        // 2) movement
        // 3) combat
        // 4) AI
        // 5) AOI visibility
        // 6) inventory/item/equipment
        // 7) outbound + persistence flush
        self.system_input(commands).await;
        self.system_movement().await;
        self.system_combat().await;
        self.system_ai().await;
        self.system_aoi().await;
        self.system_inventory().await;
    }

    async fn process_scheduled_events(&mut self) {
        if let Some(events) = self.scheduled.remove(&self.tick_index) {
            for scheduled in events {
                tracing::debug!(
                    map_id = self.config.map_id,
                    tick = self.tick_index,
                    scheduled_for = scheduled.run_at_tick,
                    "executing scheduled event"
                );
                let _ = self.outbound_tx.send(scheduled.event).await;
            }
        }
    }

    async fn system_input(&mut self, commands: Vec<IncomingCommand>) {
        for incoming in commands {
            match incoming.command {
                ClientCommand::EnterWorld => {
                    self.players
                        .entry(incoming.session_id)
                        .or_insert(PlayerState {
                            character_id: incoming.session_id,
                            x: 100,
                            y: 100,
                            hp: 100,
                            mp: 100,
                        });
                    MetricsRegistry::set(&METRICS.players_online_total, self.players.len() as u64);
                }
                ClientCommand::Move { x, y, .. } => {
                    if let Some(player) = self.players.get_mut(&incoming.session_id) {
                        player.x = x;
                        player.y = y;

                        let _ = self
                            .persist_tx
                            .send(PersistOp::SaveCharacterPosition {
                                character_id: player.character_id,
                                map_id: self.config.map_id,
                                x,
                                y,
                            })
                            .await;
                    }
                }
                ClientCommand::Chat { message } => {
                    let _ = self
                        .outbound_tx
                        .send(OutboundEvent::Text {
                            session_id: incoming.session_id,
                            text: message,
                        })
                        .await;
                }
                ClientCommand::Whisper {
                    to_character,
                    message,
                } => {
                    let _ = self
                        .outbound_tx
                        .send(OutboundEvent::Text {
                            session_id: incoming.session_id,
                            text: format!("(whisper->{to_character}) {message}"),
                        })
                        .await;
                }
                ClientCommand::GuildChat { message } => {
                    let _ = self
                        .outbound_tx
                        .send(OutboundEvent::Text {
                            session_id: incoming.session_id,
                            text: format!("[guild] {message}"),
                        })
                        .await;
                }
                ClientCommand::Attack { target_id } => {
                    self.pending_attacks.push((incoming.session_id, target_id));
                }
                ClientCommand::CastSkill {
                    skill_id,
                    target_id,
                } => {
                    self.pending_skill_casts
                        .push((incoming.session_id, skill_id, target_id));
                }
                ClientCommand::PickupItem { .. } => {
                    self.pending_inventory_changes
                        .push((incoming.session_id, "pickup_item".to_string()));
                }
                ClientCommand::DropItem { slot, quantity } => {
                    self.pending_inventory_changes.push((
                        incoming.session_id,
                        format!("drop_item(slot={slot},qty={quantity})"),
                    ));
                }
                ClientCommand::UseItem { slot } => {
                    self.pending_inventory_changes
                        .push((incoming.session_id, format!("use_item(slot={slot})")));
                }
                ClientCommand::NpcInteraction { npc_id } => {
                    let event_tick = self.tick_index.saturating_add(1);
                    self.scheduled
                        .entry(event_tick)
                        .or_default()
                        .push(ScheduledEvent {
                            run_at_tick: event_tick,
                            event: OutboundEvent::Text {
                                session_id: incoming.session_id,
                                text: format!("npc_interaction:{npc_id}"),
                            },
                        });
                }
                ClientCommand::Heartbeat => {
                    let _ = self
                        .outbound_tx
                        .send(OutboundEvent::Text {
                            session_id: incoming.session_id,
                            text: "heartbeat_ack".to_string(),
                        })
                        .await;
                }
                ClientCommand::Logout => {
                    self.players.remove(&incoming.session_id);
                    MetricsRegistry::set(&METRICS.players_online_total, self.players.len() as u64);
                }
                _ => {
                    // other commands are processed by dedicated systems
                }
            }
        }
    }

    async fn system_movement(&mut self) {
        for (session_id, player) in &self.players {
            let _ = self
                .outbound_tx
                .send(OutboundEvent::Position {
                    session_id: *session_id,
                    x: player.x,
                    y: player.y,
                })
                .await;
        }
    }

    async fn system_combat(&mut self) {
        let mut resolved_attacks = Vec::new();

        for (attacker, defender) in self.pending_attacks.drain(..) {
            if !self.players.contains_key(&attacker) {
                continue;
            }
            let Some(defender_state) = self.players.get_mut(&defender) else {
                continue;
            };

            let base = 5_i32;
            let variance = (self.tick_index % 4) as i32;
            let damage = base + variance;
            defender_state.hp = (defender_state.hp - damage).max(0);
            resolved_attacks.push((attacker, defender, damage, defender_state.hp == 0));
        }

        for (attacker, defender, damage, defeated) in resolved_attacks {
            let _ = self
                .outbound_tx
                .send(OutboundEvent::CombatResult {
                    attacker,
                    defender,
                    damage,
                })
                .await;
            let _ = self
                .persist_tx
                .send(PersistOp::SaveCombatLog {
                    attacker,
                    defender,
                    map_id: self.config.map_id,
                    damage,
                })
                .await;

            if defeated {
                let _ = self
                    .outbound_tx
                    .send(OutboundEvent::Text {
                        session_id: defender,
                        text: "you are defeated".to_string(),
                    })
                    .await;
            }
        }

        let mut resolved_skills = Vec::new();
        for (caster, skill_id, target_id) in self.pending_skill_casts.drain(..) {
            let Some(caster_state) = self.players.get_mut(&caster) else {
                continue;
            };
            if caster_state.mp < 10 {
                let _ = self
                    .outbound_tx
                    .send(OutboundEvent::Text {
                        session_id: caster,
                        text: "not enough mana".to_string(),
                    })
                    .await;
                continue;
            }

            caster_state.mp -= 10;
            resolved_skills.push((caster, skill_id, target_id));
        }

        for (caster, skill_id, target_id) in resolved_skills {
            if let Some(target) = target_id {
                if let Some(target_state) = self.players.get_mut(&target) {
                    let damage = 10 + (skill_id.abs() % 7);
                    target_state.hp = (target_state.hp - damage).max(0);
                    let _ = self
                        .outbound_tx
                        .send(OutboundEvent::CombatResult {
                            attacker: caster,
                            defender: target,
                            damage,
                        })
                        .await;
                    let _ = self
                        .persist_tx
                        .send(PersistOp::SaveCombatLog {
                            attacker: caster,
                            defender: target,
                            map_id: self.config.map_id,
                            damage,
                        })
                        .await;
                }
            } else {
                let _ = self
                    .outbound_tx
                    .send(OutboundEvent::Text {
                        session_id: caster,
                        text: format!("skill_cast:{skill_id}"),
                    })
                    .await;
            }
        }
    }

    async fn system_ai(&mut self) {
        if !self.tick_index.is_multiple_of(100) {
            return;
        }
        if let Some((&session_id, _)) = self.players.iter().next() {
            let _ = self
                .outbound_tx
                .send(OutboundEvent::Text {
                    session_id,
                    text: "npc_ai_tick".to_string(),
                })
                .await;
        }
    }

    async fn system_aoi(&mut self) {
        if !self.tick_index.is_multiple_of(40) {
            return;
        }
        let population = self.players.len();
        for session_id in self.players.keys().copied().collect::<Vec<_>>() {
            let _ = self
                .outbound_tx
                .send(OutboundEvent::Text {
                    session_id,
                    text: format!("aoi_update:visible={population}"),
                })
                .await;
        }
    }

    async fn system_inventory(&mut self) {
        for (character_id, reason) in self.pending_inventory_changes.drain(..) {
            let _ = self
                .persist_tx
                .send(PersistOp::SaveInventoryChange {
                    character_id,
                    map_id: self.config.map_id,
                    reason,
                })
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn map_instance_consumes_commands() {
        let (tx, rx) = mpsc::channel(64);
        let (out_tx, mut out_rx) = mpsc::channel(64);
        let (persist_tx, mut persist_rx) = mpsc::channel(64);

        let instance = MapInstance::new(
            MapConfig {
                map_id: 1,
                tick_ms: 20,
                command_budget: 64,
            },
            rx,
            out_tx,
            persist_tx,
        );

        let task = tokio::spawn(instance.run());

        tx.send(IncomingCommand {
            session_id: Uuid::new_v4(),
            command: ClientCommand::EnterWorld,
        })
        .await
        .expect("send enter world");

        tx.send(IncomingCommand {
            session_id: Uuid::new_v4(),
            command: ClientCommand::Heartbeat,
        })
        .await
        .expect("send heartbeat");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let _ = out_rx.try_recv();
        let _ = persist_rx.try_recv();

        task.abort();
    }

    #[tokio::test]
    async fn attack_emits_combat_and_persist_log() {
        let (tx, rx) = mpsc::channel(64);
        let (out_tx, mut out_rx) = mpsc::channel(64);
        let (persist_tx, mut persist_rx) = mpsc::channel(64);

        let instance = MapInstance::new(
            MapConfig {
                map_id: 1,
                tick_ms: 10,
                command_budget: 64,
            },
            rx,
            out_tx,
            persist_tx,
        );

        let task = tokio::spawn(instance.run());

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        tx.send(IncomingCommand {
            session_id: a,
            command: ClientCommand::EnterWorld,
        })
        .await
        .expect("enter world a");
        tx.send(IncomingCommand {
            session_id: b,
            command: ClientCommand::EnterWorld,
        })
        .await
        .expect("enter world b");
        tx.send(IncomingCommand {
            session_id: a,
            command: ClientCommand::Attack { target_id: b },
        })
        .await
        .expect("attack");

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let mut saw_combat = false;
        while let Ok(evt) = out_rx.try_recv() {
            if matches!(evt, OutboundEvent::CombatResult { .. }) {
                saw_combat = true;
                break;
            }
        }
        assert!(saw_combat, "expected at least one combat outbound event");

        let mut saw_combat_persist = false;
        while let Ok(op) = persist_rx.try_recv() {
            if matches!(op, PersistOp::SaveCombatLog { .. }) {
                saw_combat_persist = true;
                break;
            }
        }
        assert!(saw_combat_persist, "expected combat persist op");

        task.abort();
    }

    #[tokio::test]
    async fn inventory_commands_emit_persist_ops() {
        let (tx, rx) = mpsc::channel(64);
        let (out_tx, _out_rx) = mpsc::channel(64);
        let (persist_tx, mut persist_rx) = mpsc::channel(64);

        let instance = MapInstance::new(
            MapConfig {
                map_id: 1,
                tick_ms: 10,
                command_budget: 64,
            },
            rx,
            out_tx,
            persist_tx,
        );

        let task = tokio::spawn(instance.run());

        let who = Uuid::new_v4();
        tx.send(IncomingCommand {
            session_id: who,
            command: ClientCommand::EnterWorld,
        })
        .await
        .expect("enter world");
        tx.send(IncomingCommand {
            session_id: who,
            command: ClientCommand::UseItem { slot: 1 },
        })
        .await
        .expect("use item");
        tx.send(IncomingCommand {
            session_id: who,
            command: ClientCommand::DropItem {
                slot: 2,
                quantity: 1,
            },
        })
        .await
        .expect("drop item");

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let mut changes = 0_u32;
        while let Ok(op) = persist_rx.try_recv() {
            if matches!(op, PersistOp::SaveInventoryChange { .. }) {
                changes += 1;
            }
        }
        assert!(changes >= 2, "expected at least two inventory persist ops");

        task.abort();
    }
}
