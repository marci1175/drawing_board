use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct ServerState {
    pub client_list: Arc<DashMap<SocketAddr, Client>>,
    pub canvas: Arc<DashMap<Vec<LinePos>, (f32, Color32, BrushType)>>,
}

use common_definitions::{BrushType, IndexMap, LinePos, Message, MessageType};
use dashmap::DashMap;
use egui::Color32;
use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    RecvStream, SendStream, ServerConfig,
};
use tokio::{io::AsyncReadExt, select, sync::broadcast::Receiver};
use uuid::Uuid;

pub struct Client {
    pub uuid: String,
}

pub async fn relay_message(
    mut all_client_relay: Receiver<Message>,
    mut send_stream: SendStream,
    mut client_exclusive_reciver: tokio::sync::mpsc::Receiver<MessageType>,
    server_state: ServerState,
) -> anyhow::Result<()> {
    loop {
        select! {
            received_message = all_client_relay.recv() => {
                let received_message = received_message?;

                send_stream
                    .write_all(&received_message.into_sendable())
                    .await?;
            }

            exclusive_message = client_exclusive_reciver.recv() => {
                let received_message = exclusive_message.ok_or(anyhow::Error::msg("Received an empty channel message."))?;

                //Run custom server logic and respond accordingly
                match received_message {
                    MessageType::RequestSyncLine(lines_pos_list) => {
                        match lines_pos_list {
                            Some(pos_list) => {
                                let line = server_state.canvas.get(&pos_list.iter()
                                    .map(|pos| LinePos::from(*pos))
                                    .collect::<Vec<LinePos>>());

                                let line_owned = line.map(|line| {
                                    (line.key().clone(), *line.value())
                                });

                                send_stream
                                    .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::SyncLine(common_definitions::LineSyncType::Partial(line_owned))}.into_sendable())
                                    .await?;
                            },
                            None => {
                                send_stream
                                    .write_all(&Message {
                                        uuid: Uuid::default(),
                                        msg_type: MessageType::SyncLine(common_definitions::LineSyncType::Full(IndexMap::from_iter(
                                            server_state.canvas.iter().map(|line| {
                                                (line.key().clone(), *line.value())
                                             })
                                        )
                                            )),
                                        }.into_sendable()
                                    )
                                    .await?;
                            },
                        }
                    },

                    _ => unreachable!(),
                }
            }

            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                let client = &mut send_stream;

                client
                    .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::KeepAlive}.into_sendable())
                    .await?
            }
        }
    }

    Ok(())
}

pub fn bytes_into_message(bytes: Vec<u8>) -> anyhow::Result<Message> {
    let username_buf = String::from_utf8(bytes)?;

    Ok(Message::from_str(&username_buf)?)
}

/// This function reads from the ```recv_stream``` provided as an argument.
/// It first reads a ```u64``` to decide the message's length after it reads `n` number of bytes (Indicated by the header).
/// It returns the read bytes.
pub async fn read_from_stream(recv_stream: &mut RecvStream) -> anyhow::Result<Vec<u8>> {
    let msg_length = recv_stream.read_u64().await?;

    let mut message_buffer: Vec<u8> = vec![0; msg_length as usize];

    recv_stream.read_exact(&mut message_buffer).await?;

    Ok(message_buffer)
}

/// Creates a custom ```(ServerConfig, CertificateDer<'static>)``` instance. The Certificate is insecure.
pub fn configure_server() -> anyhow::Result<(ServerConfig, CertificateDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let mut server_config =
        ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into()).unwrap();
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();

    transport_config.max_concurrent_uni_streams(0_u8.into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(2 * 60 * 60).try_into()?));

    Ok((server_config, cert_der))
}
