use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    sync::Arc,
};

use dashmap::DashMap;
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

    let (server_config, _server_cert) = configure_server()?;

    let endpoint = Endpoint::server(
        server_config,
        std::net::SocketAddr::V6(SocketAddrV6::new(addr, 3004, 0, 0)),
    )?;

    let (sx, mut rx) = mpsc::channel::<(SendStream, RecvStream, SocketAddr)>(10);

    //Spawn relay thread
    tokio::spawn(async move {
        let client_list: DashMap<SocketAddr, (SendStream, RecvStream)> = DashMap::new();

        let mut username_list: Vec<String> = vec![];

        loop {
            let incoming_client = rx.recv().await;

            if let Some(mut client) = incoming_client {
                let mut username: String = String::new();

                client.1.read_to_string(&mut username).await?;

                for mut client in client_list.iter_mut() {
                    let (client_sender, _) = client.value_mut();
                    client_sender.write_all(username.as_bytes()).await?;
                }

                username_list.push(username);

                //Send the list of the usernames to the
                client
                    .0
                    .write_all(serde_json::to_string(&username_list)?.as_bytes())
                    .await?;

                client_list.insert(client.2, (client.0, client.1));
            }
        }

        Ok::<(), anyhow::Error>(())
    });

    //Handle incoming requests
    loop {
        let sx = sx.clone();

        //Wait for an incoming connection
        let inbound_connection = endpoint.accept().await;

        //Spawn async thread
        tokio::spawn(async move {
            let incoming_connection = inbound_connection.unwrap();

            let connection = incoming_connection.await?;

            let (sendstream, recvstream) = connection.accept_bi().await?;

            sx.send((sendstream, recvstream, connection.remote_address()))
                .await
                .unwrap();

            Ok::<(), anyhow::Error>(())
        });
    }
}

pub fn configure_server() -> anyhow::Result<(ServerConfig, CertificateDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let mut server_config =
        ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}
