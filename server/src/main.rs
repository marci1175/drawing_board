use std::{
    net::{IpAddr, Ipv6Addr, SocketAddrV6},
    sync::Arc,
};

use quinn::{
    rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    Endpoint, EndpointConfig, ServerConfig,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(not(debug_assertions))]
    let addr = Ipv6Addr::UNSPECIFIED;

    #[cfg(debug_assertions)]
    let addr = Ipv6Addr::LOCALHOST;

    let (server_config, server_cert) = configure_server()?;

    let endpoint = Endpoint::server(
        server_config,
        std::net::SocketAddr::V6(SocketAddrV6::new(addr, 3004, 0, 0)),
    )?;

    loop {
        //Wait for an incoming connection
        let inbound_connection = endpoint.accept().await;

        //Spawn async thread
        let _: tokio::task::JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
            let incoming_connection = inbound_connection.unwrap();
        
            let connection = incoming_connection.await?;

            let (recv_stream, send_stream) = connection.accept_bi().await?;

            

            Ok(())
        });
    }

    Ok(())
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
