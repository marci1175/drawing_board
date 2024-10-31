use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use common_definitions::{Message, MessageType};
use dashmap::DashMap;
use drawing_board_server::Client;
use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    Endpoint, RecvStream, SendStream, ServerConfig,
};
use tokio::{
    io::AsyncReadExt,
    select,
    sync::{
        broadcast::{self, Sender},
        mpsc,
    },
};
use uuid::Uuid;

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

    let (relay_sender, mut relay_reciver) = broadcast::channel::<Message>(100);
    let client_list: Arc<DashMap<SocketAddr, Client>> = Arc::new(DashMap::new());

    let client_list_clone = client_list.clone();

    //Spawn client registering thread
    tokio::spawn(async move {
        let mut username_uuid_pair_list: Vec<(String, uuid::Uuid)> = vec![];

        loop {
            let incoming_client = rx.recv().await;

            if let Some(mut client) = incoming_client {
                if let Ok(byte_buf) = read_from_stream(&mut client.1).await {
                    let username_buf =
                    String::from_utf8(byte_buf).unwrap();

                let message = Message::from_str(&username_buf).unwrap();
                let uuid = message.uuid;
                let inner_message = message.msg_type;

                for mut client in client_list_clone.iter_mut() {
                    let client_key = *client.key();

                    let client_sender = &mut client.value_mut().send_stream;

                    //If we get an error it is probably because the client had disconnected
                    if let Err(_err) = client_sender
                        .write_all(
                            &Message {
                                msg_type: inner_message.clone(),
                                uuid,
                            }
                            .into_sendable(),
                        )
                        .await
                    {
                        client_list_clone.remove(&client_key);
                    };
                }

                if let MessageType::Connecting(username) = inner_message {
                    username_uuid_pair_list.push((username, uuid));
                }

                //Send the list of the usernames to the
                client
                    .0
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
                    .unwrap();

                client_list_clone.insert(
                    client.2,
                    Client {
                        uuid: uuid.to_string(),
                        send_stream: client.0,
                    },
                );

                spawn_client_listener(relay_sender.clone(), client.1);
                }
            }
        }
    });

    //Spawn relay thread
    tokio::spawn(async move {
        loop {
            select! {
                recived_message = relay_reciver.recv() => {
                    match recived_message {
                        Ok(recived_message) => {
                            for mut client_row in client_list.iter_mut() {
                                let client = &mut client_row.send_stream;

                                client
                                    .write_all(&recived_message.into_sendable())
                                    .await
                                    .unwrap();
                            }
                        }
                        Err(err) => {
                            dbg!(err);
                        }
                    }
                }

                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    for mut client_row in client_list.iter_mut() {
                        let client = &mut client_row.send_stream;

                        client
                            .write_all(&Message {uuid: Uuid::default(), msg_type: MessageType::KeepAlive}.into_sendable())
                            .await
                            .unwrap();
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

/// This function creates a listener thread, from the ```recv_stream``` provided as an argument.
/// All recived messages are sent to the relay (```Sender<Message>```) channel so that the relay thread can realy the message to all of the clients.
pub fn spawn_client_listener(relay: Sender<Message>, mut recv_stream: RecvStream) {
    tokio::spawn(async move {
        loop {
            if let Ok(message_buffer) = read_from_stream(&mut recv_stream).await {
                let message = Message::from_str(&String::from_utf8(message_buffer).unwrap()).unwrap();

            if !matches!(message.msg_type, MessageType::KeepAlive) {
                relay.send(message).unwrap();
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
