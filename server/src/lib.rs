use quinn::SendStream;

pub struct Client {
    pub uuid: String,
    pub send_stream: SendStream,
}
