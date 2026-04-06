use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCommand {
    SayMap {
        from_character_id: Uuid,
        map_id: i32,
        message: String,
    },
    Whisper {
        from_character_id: Uuid,
        to_character_name: String,
        message: String,
    },
    Guild {
        from_character_id: Uuid,
        guild_id: Uuid,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEnvelope {
    pub from_character_id: Uuid,
    pub message: String,
    pub channel: String,
}

pub struct ChatService {
    pub rx: mpsc::Receiver<ChatCommand>,
    pub tx: mpsc::Sender<ChatEnvelope>,
}

impl ChatService {
    pub async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            let envelope = match cmd {
                ChatCommand::SayMap {
                    from_character_id,
                    message,
                    ..
                } => ChatEnvelope {
                    from_character_id,
                    message,
                    channel: "map".to_string(),
                },
                ChatCommand::Whisper {
                    from_character_id,
                    message,
                    ..
                } => ChatEnvelope {
                    from_character_id,
                    message,
                    channel: "whisper".to_string(),
                },
                ChatCommand::Guild {
                    from_character_id,
                    message,
                    ..
                } => ChatEnvelope {
                    from_character_id,
                    message,
                    channel: "guild".to_string(),
                },
            };

            let _ = self.tx.send(envelope).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn say_map_produces_map_envelope() {
        let (cmd_tx, cmd_rx) = mpsc::channel(4);
        let (out_tx, mut out_rx) = mpsc::channel(4);

        let task = tokio::spawn(
            ChatService {
                rx: cmd_rx,
                tx: out_tx,
            }
            .run(),
        );

        let sender = Uuid::new_v4();
        cmd_tx
            .send(ChatCommand::SayMap {
                from_character_id: sender,
                map_id: 1,
                message: "hello".to_string(),
            })
            .await
            .expect("send chat command");

        let envelope = out_rx.recv().await.expect("chat envelope");
        assert_eq!(envelope.from_character_id, sender);
        assert_eq!(envelope.channel, "map");
        assert_eq!(envelope.message, "hello");

        task.abort();
    }
}
