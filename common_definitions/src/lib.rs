#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MessageType {
    /// This enum contains the list of the connected user's username
    ClientList(Vec<String>),
    /// This enum contains the username of the user who has their cursor's position at (f32, f32)
    CursorPosition(f32, f32),
}
