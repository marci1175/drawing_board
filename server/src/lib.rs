use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct ServerState {
    pub client_list: Arc<DashMap<SocketAddr, Client>>,
    pub canvas: Arc<DashMap<Vec<LinePos>, Brush>>,
}

use common_definitions::{Brush, CancellationToken, LinePos, Message, MessageType};
use dashmap::DashMap;
use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    RecvStream, SendStream, ServerConfig,
};
use tokio::{
    io::AsyncReadExt,
    select,
    sync::broadcast::{Receiver, Sender},
};
use tracing::{event, Level};
use uuid::Uuid;

pub struct Client {
    pub uuid: String,
}

pub fn bytes_into_message(bytes: Vec<u8>) -> anyhow::Result<Message> {
    let username_buf = String::from_utf8(bytes)?;

    Ok(Message::from_str(&username_buf)?)
}

/// This function reads from the ```recv_stream``` provided as an argument.
/// It first reads a ```u64``` to decide the message's length after it reads `n` number of bytes (Indicated by the header).
/// It returns the read bytes.
pub async fn read_from_stream(recv_stream: &mut RecvStream) -> anyhow::Result<Vec<u8>> {
    // Fetch message length by getting the message's header
    let msg_length = recv_stream.read_u64().await?;

    // Allocate the message's buffer
    let mut message_buffer: Vec<u8> = vec![0; msg_length as usize];

    // Load the message's bytes into the buffer
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

/// This function creates a listener thread, from the ```recv_stream``` provided as an argument.
/// All recived messages are sent to the relay (```Sender<Message>```) channel so that the relay thread can realy the message to all of the clients.
/// If an invalid message is recieved this function will automaticly cancel the `client_shutdown_token`.
pub fn spawn_client_listener(
    // This is used to send messages to the sender thread so the message sent to this channel will be sent to all the connected clients.
    relay: Sender<Message>,
    // This `RecvStream` is used for reciving messages from the remote client
    recv_stream: RecvStream,
    // This channel side is for sending messages to the canvas writer.
    // This sender only accepts `MessageType: ModifiyLine, AddLine`
    canvas_sender: tokio::sync::mpsc::Sender<MessageType>,
    // This channel is used to send messages to the remote client
    client_exclusive_sender: tokio::sync::mpsc::Sender<MessageType>,
    // This `CancellationToken` is used to cancel the listener thread if the sender thread panics / fails.
    client_shutdown_token: CancellationToken,

    client_address: SocketAddr,

    server_state: ServerState,
) {
    //Spawn thread
    tokio::spawn(async move {
        if let Err(err) = listen_for_message(
            recv_stream,
            relay,
            canvas_sender,
            client_exclusive_sender,
            client_shutdown_token.clone(),
            client_address,
        )
        .await
        {
            //Shutdown both sender and listener
            client_shutdown_token.cancel();

            server_state.client_list.remove(&client_address);

            //Display error
            event!(
                Level::INFO,
                "Client disconnected, shutting down thread: {err}"
            );
        }
    });
}

/// Listens for messages from the client.
pub async fn listen_for_message(
    mut recv_stream: RecvStream,
    relay: Sender<Message>,
    canvas_sender: tokio::sync::mpsc::Sender<MessageType>,
    client_exclusive_sender: tokio::sync::mpsc::Sender<MessageType>,
    client_shutdown_token: CancellationToken,
    client_address: SocketAddr,
) -> anyhow::Result<()> {
    loop {
        event!(
            Level::INFO,
            "Listening for a message from: {client_address}."
        );
        select! {
            message_buffer = read_from_stream(&mut recv_stream) => {
                // Read the message bytes
                let buffer = message_buffer?;

                    //Turn the bytes into a `Message` instance
                    let message = bytes_into_message(buffer)?;

                    //Match the `MessageType` types
                    match message.msg_type.clone() {
                        // These messages can be sent to all the connected clients
                        MessageType::ClientList(_)
                        | MessageType::CursorPosition(_)
                        | MessageType::Connecting(_)
                        | MessageType::Disconnecting => {
                            relay.send(message)?;
                        }

                        //These are sent to the Canvas writer to be backed up and to all of the clients.
                        MessageType::ModifyLine(_) | MessageType::AddLine(_) => {
                            canvas_sender.send(message.msg_type.clone()).await?;
                            relay.send(message)?;
                        }

                        // If the server recieves a `KeepAlive` message it should echo it back to the client
                        MessageType::KeepAlive => {
                            client_exclusive_sender.send(MessageType::KeepAlive).await?;
                        },

                        // When a `LineSync` is requested the server should exclusively reply to the client who requested the `Sync`
                        MessageType::RequestSyncLine(_) => {
                            client_exclusive_sender
                                .send(message.msg_type)
                                .await
                                ?;
                        }

                        // This message can only be sent by the server. Client issue.
                        MessageType::SyncLine(_) => {
                            event!(Level::ERROR, "The client can't send this message");
                        }
                    }
            }
            _ = client_shutdown_token.cancelled() => {
                event!(Level::INFO, "Shut down client: {client_address} listener.");
                break
            },
        }
    }

    Ok(())
}

/// This function spawns thread with a `relay_message` function running. If an error occurs this function will automatcily cancel the `client_shutdown_token`
pub fn spawn_client_sender(
    relay: Receiver<Message>,
    send_stream: SendStream,
    client_exclusive_reciver: tokio::sync::mpsc::Receiver<MessageType>,
    server_state: ServerState,
    client_shutdown_token: CancellationToken,
    client_address: SocketAddr,
) {
    tokio::spawn(async move {
        if let Err(err) = relay_message(
            relay,
            send_stream,
            client_exclusive_reciver,
            server_state.clone(),
            client_shutdown_token.clone(),
            client_address,
        )
        .await
        {
            //Shutdown both sender and listener
            client_shutdown_token.cancel();

            server_state.client_list.remove(&client_address);

            //Display error
            event!(
                Level::INFO,
                "Client disconnected, shutting down thread: {err}"
            );
        }
    });
}

/// Relays messages to the client.
pub async fn relay_message(
    mut all_client_relay: Receiver<Message>,
    mut send_stream: SendStream,
    mut client_exclusive_reciver: tokio::sync::mpsc::Receiver<MessageType>,
    server_state: ServerState,
    client_shutdown_token: CancellationToken,
    client_address: SocketAddr,
) -> anyhow::Result<()> {
    loop {
        select! {
            received_message = all_client_relay.recv() => {
                event!(Level::INFO, "Received global client message from: {client_address}.");

                let received_message = received_message?;

                send_stream
                    .write_all(&received_message.into_sendable())
                    .await?;
            }

            exclusive_message = client_exclusive_reciver.recv() => {
                event!(Level::INFO, "Received client exclusive message from: {client_address}.");

                let received_message = exclusive_message.ok_or(anyhow::Error::msg("Received an empty channel message."))?;

                //Run custom server logic and respond accordingly
                match received_message {
                    MessageType::RequestSyncLine(lines_pos_list) => {
                        match lines_pos_list {
                            Some(pos_list) => {
                                let line = server_state.canvas.get(&pos_list.to_vec());

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
                                        msg_type: MessageType::SyncLine(common_definitions::LineSyncType::Full(Vec::from_iter(
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

                    MessageType::KeepAlive => {
                        send_stream
                                    .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::KeepAlive}.into_sendable())
                                    .await?;
                        event!(Level::TRACE, "Sent KeepAlive message to: {client_address}.");
                    }

                    _ => unreachable!(),
                }
            }

            _ = client_shutdown_token.cancelled() => break,
        }
        event!(Level::INFO, "Relayed message to: {client_address}.");
    }

    Ok(())
}
