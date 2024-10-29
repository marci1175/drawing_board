use quinn::SendStream;

pub struct Client {
    pub username: String,
    pub send_stream: SendStream,
}
