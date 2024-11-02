use egui::{Color32, Pos2};
use std::{fmt::Display, str::FromStr};
use strum::{EnumCount, IntoStaticStr};
pub use uuid::Uuid;

/// The message types the client and the server can send.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MessageType {
    /// This enum contains the list of the connected user's username
    ClientList(Vec<(String, Uuid)>),
    /// This enum contains the username of the user who has their cursor's position
    CursorPosition(Pos2),
    /// This enum contains the username of the user who has connected to the server.
    Connecting(String),
    /// This enum indicated a user disconnect
    Disconnecting,
    /// This enum is used as a ```KeepAlive``` packet so that the `QUIC` connection doesn't time out.
    KeepAlive,

    AddLine((Vec<Pos2>, (f32, Color32, BrushType))),
    ModifyLine((Vec<Pos2>, Option<(f32, Color32, BrushType)>)),
    RequestSyncLine(Option<Vec<Pos2>>),

    SyncLine(LineSyncType),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum LineSyncType {
    Full(Vec<(Vec<Pos2>, (f32, Color32, BrushType))>),
    Partial(Option<(Vec<Pos2>, (f32, Color32, BrushType))>),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LineData {
    pub line_pos2: Vec<Pos2>,

    pub line_modification: Option<(f32, Color32, BrushType)>,
}

/// The types of brushes the client can display.
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Default,
    PartialEq,
    Clone,
    Copy,
    EnumCount,
    IntoStaticStr,
    Debug,
)]
pub enum BrushType {
    None,
    Graffiti,
    Pencil,
    #[default]
    Marker,
    Eraser,
}

pub const BRUSH_TYPE_COUNT: usize = BrushType::COUNT;

/// The types of tabs this application supports.
#[derive(
    IntoStaticStr, Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Hash, PartialEq, Eq,
)]
pub enum TabType {
    /// Used for showing the actual Canvas the user can paint at.
    Canvas,
    /// Used for displaying the Brush's settings the user can paint on the canvas with.
    BrushSettings,
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
    type Err = serde_json::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
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
