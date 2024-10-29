use std::{fmt::Display, str::FromStr};

use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MessageType {
    /// This enum contains the list of the connected user's username
    ClientList(Vec<(String, Uuid)>),
    /// This enum contains the username of the user who has their cursor's position at (f32, f32)
    CursorPosition(f32, f32),

    Connecting(String),
    Disconnecting,
    KeepAlive,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Message {
    pub uuid: Uuid,
    pub msg_type: MessageType,
}

impl Message {
    pub fn to_serde_string(uuid: Uuid, msg_type: MessageType) -> Self {
        Self { uuid, msg_type }
    }
}

impl FromStr for Message {
    type Err = Box<dyn std::error::Error>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl Message {
    pub fn into_sendable(&self) -> Vec<u8> {
        let mut message = self.to_string().as_bytes().to_vec();

        let mut message_header = (message.len() as u64).to_be_bytes().to_vec();

        message_header.append(&mut message);

        message_header
    }
}
