use quinn::{RecvStream, SendStream};

pub struct Client {
    pub username: String,
    pub send_stream: SendStream,
    pub recv_stream: RecvStream,
}
