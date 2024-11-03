use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    sync::Arc,
};

use common_definitions::{CancellationToken, Message, MessageType};
use dashmap::DashMap;
use drawing_board_server::{
    bytes_into_message, configure_server, read_from_stream, spawn_client_listener,
    spawn_client_sender, Client, ServerState,
};
use quinn::{Endpoint, RecvStream, SendStream};
use tokio::sync::{
    broadcast::{self},
    mpsc::{self, channel},
};

/* TODO:
    implement tracing / logging.
*/

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

    // Create the relay channel pair
    // This is used to broadcast a message to all of the clients.
    let (relay_sender, relay_reciver) = broadcast::channel::<Message>(100);

    // This channel is used to send messages to the canvas writer, which writes information to the server's internal storage
    let (canvas_sender, mut canvas_receiver) = channel::<MessageType>(1000);

    // Create a `ServerState` instance to store the servers state
    let server_state = ServerState {
        client_list: Arc::new(DashMap::new()),
        canvas: Arc::new(DashMap::new()),
    };

    //Clone the client list's handle
    let client_list_clone = server_state.client_list.clone();

    //Clone the server_state variable
    let server_state_clone = server_state.clone();

    tokio::spawn(async move {
        loop {
            if let Some(message) = canvas_receiver.recv().await {
                match message {
                    MessageType::AddLine((pos, props)) => {
                        server_state.canvas.insert(pos.to_vec(), props);
                    }
                    MessageType::ModifyLine((pos, line_property_change)) => {
                        match line_property_change {
                            // The line gets modified
                            Some(props) => {
                                if let Some(mut line_props) =
                                    server_state.canvas.get_mut(&pos.to_vec())
                                {
                                    let line_props = line_props.value_mut();

                                    *line_props = props;
                                } else {
                                    eprintln!("Client/Server desync");
                                }
                            }
                            // The line gets deleted
                            None => {
                                server_state.canvas.remove(&pos.to_vec());
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

                //Read `n` number of bytes from the stream
                if let Ok(byte_buf) = read_from_stream(&mut recv_stream).await {
                    //Convert a list of bytes into a `Message`
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

                            //Create client exlusive channels these are used to send messages to the client who has created this set of channels exclusively
                            let (client_exclusive_sender, client_exclusive_listener) =
                                channel::<MessageType>(100);

                            //Create a cancellation token so that if either the listener or the sender fail it will shut down both threads.
                            let client_cancellation_token = CancellationToken::new();

                            // Spawn client listener thread
                            spawn_client_listener(
                                relay_sender.clone(),
                                recv_stream,
                                canvas_sender.clone(),
                                client_exclusive_sender,
                                client_cancellation_token.clone(),
                            );

                            //Spawn cleint relay thread
                            spawn_client_sender(
                                relay_reciver.resubscribe(),
                                send_stream,
                                client_exclusive_listener,
                                server_state_clone.clone(),
                                client_cancellation_token,
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
