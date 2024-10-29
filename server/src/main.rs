use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    sync::Arc,
};

use dashmap::DashMap;
use drawing_board_server::Client;
use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    Endpoint, RecvStream, SendStream, ServerConfig,
};
use tokio::{io::AsyncReadExt, sync::mpsc};

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

    //Spawn relay thread
    tokio::spawn(async move {
        let client_list: DashMap<SocketAddr, Client> = DashMap::new();

        let mut username_list: Vec<String> = vec![];

        loop {
            let incoming_client = rx.recv().await;

            if let Some(mut client) = incoming_client {
                let mut username: String = String::new();

                client.1.read_to_string(&mut username).await.unwrap();

                for mut client in client_list.iter_mut() {
                    let client_key = *client.key();

                    let client_sender = &mut client.value_mut().send_stream;

                    //If we get an error it is probably because the client had disconnected
                    if let Err(_err) = client_sender.write_all(username.as_bytes()).await {
                        client_list.remove(&client_key);
                    };

                    client_sender.finish().unwrap();
                }

                username_list.push(username.clone());

                //Send the list of the usernames to the
                client
                    .0
                    .write_all(serde_json::to_string(&username_list).unwrap().as_bytes())
                    .await
                    .unwrap();

                client.0.finish().unwrap();

                client_list.insert(
                    client.2,
                    Client {
                        username,
                        send_stream: client.0,
                        recv_stream: client.1,
                    },
                );
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

            let (sendstream, recvstream) = connection.accept_bi().await.unwrap();

            sx.send((sendstream, recvstream, connection.remote_address()))
                .await
                .unwrap();
        });
    }
}

pub fn configure_server() -> anyhow::Result<(ServerConfig, CertificateDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let mut server_config =
        ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into()).unwrap();
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}
