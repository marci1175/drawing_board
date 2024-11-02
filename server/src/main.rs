use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use common_definitions::{BrushType, Message, MessageType};
use dashmap::DashMap;
use drawing_board_server::Client;
use egui::{Color32, Pos2};
use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    Endpoint, RecvStream, SendStream, ServerConfig,
};
use tokio::{
    io::AsyncReadExt,
    select,
    sync::{
        broadcast::{self, Receiver, Sender},
        mpsc::{self, channel},
    },
};
use uuid::Uuid;

#[derive(Clone)]
pub struct ServerState {
    client_list: Arc<DashMap<SocketAddr, Client>>,
    canvas: Arc<DashMap<Vec<LinePos>, (f32, Color32, BrushType)>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(not(debug_assertions))]
    let addr = Ipv6Addr::UNSPECIFIED;

    #[cfg(debug_assertions)]
    let addr = Ipv6Addr::LOCALHOST;

    let (server_config, _server_cert) = configure_server().unwrap();

    let endpoint = Endpoint::server(
        server_config,
        std::net::SocketAddr::V6(SocketAddrV6::new(addr, 3004, 0, 0)),
    )
    .unwrap();

    let (sx, mut rx) = mpsc::channel::<(SendStream, RecvStream, SocketAddr)>(10);

    let (relay_sender, relay_reciver) = broadcast::channel::<Message>(100);

    let (canvas_sender, mut canvas_receiver) = channel::<MessageType>(1000);

    let server_state = ServerState {
        client_list: Arc::new(DashMap::new()),
        canvas: Arc::new(DashMap::new()),
    };

    let client_list_clone = server_state.client_list.clone();
    let server_state_clone = server_state.clone();

    tokio::spawn(async move {
        loop {
            if let Some(message) = canvas_receiver.recv().await {
                match message {
                    MessageType::AddLine((pos, props)) => {
                        server_state
                            .canvas
                            .insert(pos.iter().map(|pos| LinePos::from(*pos)).collect(), props);
                    }
                    MessageType::ModifyLine((pos, line_property_change)) => {
                        match line_property_change {
                            // The line gets modified
                            Some(props) => {
                                if let Some(mut line_props) = server_state.canvas.get_mut(
                                    &pos.iter()
                                        .map(|pos| LinePos::from(*pos))
                                        .collect::<Vec<LinePos>>(),
                                ) {
                                    let line_props = line_props.value_mut();

                                    *line_props = props;
                                } else {
                                    eprintln!("Client/Server desync");
                                }
                            }
                            // The line gets deleted
                            None => {
                                server_state.canvas.remove(
                                    &pos.iter()
                                        .map(|pos| LinePos::from(*pos))
                                        .collect::<Vec<LinePos>>(),
                                );
                            }
                        }
                    }

                    _ => unreachable!(),
                }
            }
        }
    });

    //Spawn client registering thread
    tokio::spawn(async move {
        let mut username_uuid_pair_list: Vec<(String, uuid::Uuid)> = vec![];

        loop {
            let incoming_client = rx.recv().await;

            if let Some(client) = incoming_client {
                let (mut send_stream, mut recv_stream, client_address) = client;

                if let Ok(byte_buf) = read_from_stream(&mut recv_stream).await {
                    match bytes_into_message(byte_buf) {
                        Ok(message) => {
                            let uuid = message.uuid;
                            let inner_message = message.msg_type;

                            if let MessageType::Connecting(username) = inner_message {
                                username_uuid_pair_list.push((username, uuid));
                            }

                            // Send the list of the usernames to the connected client
                            // If this write fails, that means that the client has already disconnected, this is unexpected behavior from the client.
                            if let Err(err) = send_stream
                                .write_all(
                                    &common_definitions::Message::new(
                                        uuid,
                                        common_definitions::MessageType::ClientList(
                                            username_uuid_pair_list.clone(),
                                        ),
                                    )
                                    .into_sendable(),
                                )
                                .await
                            {
                                eprintln!("Client unexpectededly disconnected: {err}");
                            }

                            //Save client's send_stream and address
                            client_list_clone.insert(
                                client_address,
                                Client {
                                    uuid: uuid.to_string(),
                                },
                            );

                            let (client_exclusive_sender, client_exclusive_listener) =
                                channel::<MessageType>(100);

                            spawn_client_listener(
                                relay_sender.clone(),
                                recv_stream,
                                canvas_sender.clone(),
                                client_exclusive_sender,
                            );

                            //Spawn relay thread
                            spawn_client_sender(
                                relay_reciver.resubscribe(),
                                send_stream,
                                client_exclusive_listener,
                                server_state_clone.clone(),
                            );
                        }
                        Err(err) => {
                            eprintln!("Received malformed input. Disconnecting client.");
                            eprintln!("{err}");

                            client_list_clone.remove(&client_address);
                        }
                    }
                }
            }
        }
    });

    //Handle incoming requests
    loop {
        let sx = sx.clone();

        //Wait for an incoming connection
        let inbound_connection = endpoint.accept().await;

        //Spawn async thread
        tokio::spawn(async move {
            let incoming_connection = inbound_connection
                .ok_or_else(|| anyhow::Error::msg("Client closed connection"))
                .unwrap();

            let connection = incoming_connection.await.unwrap();

            dbg!(connection.remote_address());

            let (sendstream, recvstream) = connection.accept_bi().await.unwrap();

            sx.send((sendstream, recvstream, connection.remote_address()))
                .await
                .unwrap();
        });
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct LinePos {
    x: String,
    y: String,
}

impl From<Pos2> for LinePos {
    fn from(value: Pos2) -> Self {
        Self {
            x: value.x.to_string(),
            y: value.y.to_string(),
        }
    }
}

pub fn spawn_client_sender(
    relay: Receiver<Message>,
    send_stream: SendStream,
    client_exclusive_reciver: tokio::sync::mpsc::Receiver<MessageType>,
    server_state: ServerState,
) {
    tokio::spawn(async move {
        if let Err(err) =
            relay_message(relay, send_stream, client_exclusive_reciver, server_state).await
        {
            panic!("Client disconnected: {err}");
        }
    });
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
                                    let pos2_list: Vec<Pos2> = line.key().iter().map(|pos| Pos2::new(pos.x.parse().unwrap(), pos.y.parse().unwrap())).collect();

                                    (pos2_list, *line.value())
                                });

                                send_stream
                                    .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::SyncLine(common_definitions::LineSyncType::Partial(line_owned))}.into_sendable())
                                    .await?;
                            },
                            None => {
                                send_stream
                                    .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::SyncLine(common_definitions::LineSyncType::Full(server_state.canvas.iter().map(|line| {let pos2_list: Vec<Pos2> = line.key().iter().map(|pos| Pos2::new(pos.x.parse().unwrap(), pos.y.parse().unwrap())).collect();

                                        (pos2_list, *line.value())}).collect()))}.into_sendable())
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

/// This function creates a listener thread, from the ```recv_stream``` provided as an argument.
/// All recived messages are sent to the relay (```Sender<Message>```) channel so that the relay thread can realy the message to all of the clients.
pub fn spawn_client_listener(
    relay: Sender<Message>,
    mut recv_stream: RecvStream,
    canvas_sender: tokio::sync::mpsc::Sender<MessageType>,
    client_exclusive_sender: tokio::sync::mpsc::Sender<MessageType>,
) {
    tokio::spawn(async move {
        loop {
            if let Ok(message_buffer) = read_from_stream(&mut recv_stream).await {
                let message =
                    Message::from_str(&String::from_utf8(message_buffer).unwrap()).unwrap();

                match message.msg_type.clone() {
                    MessageType::ClientList(_)
                    | MessageType::CursorPosition(_)
                    | MessageType::Connecting(_)
                    | MessageType::Disconnecting => {
                        relay.send(message).unwrap();
                    }
                    MessageType::ModifyLine(_) | MessageType::AddLine(_) => {
                        canvas_sender.send(message.msg_type.clone()).await.unwrap();
                        relay.send(message).unwrap();
                    }
                    MessageType::KeepAlive => (),

                    MessageType::RequestSyncLine(_) => {
                        client_exclusive_sender
                            .send(message.msg_type)
                            .await
                            .unwrap();
                    }
                    MessageType::SyncLine(_) => {
                        unimplemented!("The client can't send this message")
                    }
                }
            }
        }
    });
}

/// This function reads from the ```recv_stream``` provided as an argument.
/// It first reads a ```u64``` to decide the message's length after it reads `n` number of bytes (Indicated by the header).
/// It returns the read bytes.
async fn read_from_stream(recv_stream: &mut RecvStream) -> anyhow::Result<Vec<u8>> {
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
