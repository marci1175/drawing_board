use std::{fmt::Display, str::FromStr};

use uuid::Uuid;

/// The message types the client and the server can send.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MessageType {
    /// This enum contains the list of the connected user's username
    ClientList(Vec<(String, Uuid)>),
    /// This enum contains the username of the user who has their cursor's position at (f32, f32)
    CursorPosition(f32, f32),
    /// This enum contains the username of the user who has connected to the server.
    Connecting(String),
    /// This enum indicated a user disconnect
    Disconnecting,
    /// This enum is used as a ```KeepAlive``` packet so that the `QUIC` connection doesn't time out.
    KeepAlive,
}

/// The message wrapper.
/// This struct contains the uuid of the sender and the actual message.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Message {
    /// The ```Uuid``` of the sender
    pub uuid: Uuid,
    /// The inner message.
    pub msg_type: MessageType,
}

impl Message {
    pub fn new(uuid: Uuid, msg_type: MessageType) -> Self {
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
    /// This function creates a buffer from the called upon ```Message``` instance.
    /// It also appends a message length header to the front of the message bytes.
    pub fn into_sendable(&self) -> Vec<u8> {
        let mut message = self.to_string().as_bytes().to_vec();

        let mut message_header = (message.len() as u64).to_be_bytes().to_vec();

        message_header.append(&mut message);

        message_header
    }
}
