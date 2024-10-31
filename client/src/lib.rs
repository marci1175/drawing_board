pub const DRAWING_BOARD_IMAGE_EXT: &str = "dbimg";
pub const DRAWING_BOARD_WORKSPACE_EXT: &str = "dbproject";
use chrono::{Local, NaiveDate};
use common_definitions::{BrushType, Message, MessageType, TabType, BRUSH_TYPE_COUNT};
use egui::{
    ahash::{HashSet, HashSetExt},
    util::undoer::Undoer,
    Color32, Pos2,
};
use egui_dock::{DockState, SurfaceIndex};
use quinn::{
    crypto::rustls::QuicClientConfig,
    rustls::{
        self,
        pki_types::{CertificateDer, ServerName, UnixTime},
    },
    ClientConfig, Connection, Endpoint, RecvStream, SendStream,
};
use serde::Deserialize;
use std::{
    collections::HashMap, fs, net::Ipv6Addr, path::PathBuf, str::FromStr, sync::Arc, time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    select,
    sync::{
        mpsc::{channel, Sender},
        Mutex, RwLock,
    },
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
mod app;

pub type BrushMap = Vec<(Vec<Pos2>, (f32, Color32, BrushType))>;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ApplicationContext {
    lines: BrushMap,
    paintbrush: PaintBrushes,

    file_session: Option<FileSession>,

    undoer: Undoer<BrushMap>,
    open_tabs: HashSet<TabType>,

    connection: ConnectionData,

    export_path: Option<PathBuf>,
}

/// This struct contains the information useful for the connection process (Like ```username```, ```target_address``` and ```connected_clients```), and the current session connected to the server.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ConnectionData {
    /// The target address of the server we would like to connect to
    target_address: String,

    /// The username of the client
    /// This is manually set before connection
    username: String,

    /// The session reciver channel used to recive the ```ConnectionSession``` instance from the async connecting thread
    #[serde(skip)]
    session_reciver: Option<std::sync::mpsc::Receiver<ConnectionSession>>,

    /// The list of the connected clients' username and last known cursor position
    #[serde(skip)]
    connected_clients: HashMap<Uuid, (String, Pos2)>,

    /// The current open session to the server available at the ```target_address```
    #[serde(skip)]
    current_session: Option<ConnectionSession>,
}

/// The current connection session to the remote address/server.
pub struct ConnectionSession {
    /// The ```CancellationToken``` to the threads ensuring connection to said server.
    pub connection_cancellation_token: CancellationToken,

    /// The handle to the remote address.
    pub connection_handle: Arc<RwLock<Connection>>,

    /// The send stream to the remote address.
    /// This is used to send data to the server.
    pub send_stream: Arc<Mutex<SendStream>>,

    /// The recive stream from the remote address.
    /// This is used to recive data from the server.
    pub recv_stream: Arc<Mutex<RecvStream>>,

    /// This ```Sender``` channel side is used to send ```MessageType```-s to the sender thread (Cancellable via ```connection_cancellation_token```).
    pub sender_to_server: Sender<MessageType>,

    /// This ```Reciver``` channel side is used to receive messages from the server (Cancellable via ```connection_cancellation_token```).
    pub message_reciver_from_server: tokio::sync::mpsc::Receiver<Message>,
}

impl ConnectionSession {
    /// This function cancels the ```connection_cancellation_token``` ending all threads receiving or sending messages to the server.
    pub fn cancel_connection(&self) {
        self.connection_cancellation_token.cancel();
    }
}

/// This struct contains useful infromation about the current file session.
/// This struct is initalized when opening a file, containing its properties.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct FileSession {
    /// The ```PathBuf``` to the file.
    pub file_path: PathBuf,
    /// The project's name.
    pub project_name: String,
    /// The date this project was created (```NaiveDate```).
    pub project_created: NaiveDate,
}

impl FileSession {
    pub fn create_session(file_path: PathBuf, project_name: String) -> Self {
        Self {
            file_path,
            project_name,
            project_created: Local::now().date_naive(),
        }
    }
}

/// This struct contains the properties of the whole application.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Application {
    /// The ```Dockstate<TabType>``` of the application.
    /// ```TabType``` is the types of tabs the application has.
    tree: DockState<TabType>,

    /// This field contains all information related to the runtime.
    /// This field does not inherently contain information which can directly affect the Application's runtime.
    context: ApplicationContext,

    /// ```Uuid``` of the user who has opened this application.
    /// A new ```Uuid``` instance is created whenever the application is opened.
    #[serde(skip)]
    uuid: ClientIdentificator,
}

/// This struct wraps the Uuid so that a custom default can be implemented for it.
pub struct ClientIdentificator(Uuid);

impl Default for ClientIdentificator {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Application {
    /// Resets the application's state by replacing it with ```Application::default()```.
    pub fn reset(&mut self) {
        *self = Application::default();
    }
}

/// Custom certificate, this doesnt verify anything I should implement a working one.
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

/// Trait implementation for the custom certificate struct
impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// This function connects to the ```target_address``` the client has provided.
/// This function first sends a ```Message {msg_type: common_definitions::MessageType::Connecting(username), uuid}``` packet, which is then relayed to all connected clients.
/// This way everyone can pair the username with the provided uuid.
pub async fn connect_to_server(
    target_address: String,
    username: String,
    uuid: Uuid,
) -> anyhow::Result<ConnectionSession> {
    let mut endpoint: Endpoint = Endpoint::client((Ipv6Addr::UNSPECIFIED, 0).into())?;

    endpoint.set_default_client_config(ClientConfig::new(Arc::new(QuicClientConfig::try_from(
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    )?)));

    let client: quinn::Connection = endpoint
        .connect(target_address.parse()?, "localhost")?
        .await?;

    let (mut send_stream, recv_stream) = client.clone().open_bi().await?;

    let connection_cancellation_token = CancellationToken::new();

    //Send username
    send_stream
        .write_all(
            &Message::new(
                uuid,
                common_definitions::MessageType::Connecting(username.clone()),
            )
            .into_sendable(),
        )
        .await
        .unwrap();

    let send_stream = Arc::new(Mutex::new(send_stream));
    let recv_stream = Arc::new(Mutex::new(recv_stream));
    let session = ConnectionSession {
        connection_cancellation_token: connection_cancellation_token.clone(),
        send_stream: send_stream.clone(),
        recv_stream: recv_stream.clone(),
        connection_handle: Arc::new(RwLock::new(client)),
        sender_to_server: {
            let (pos_sender, mut pos_reciver) = channel::<MessageType>(255);
            let connection_cancellation_token_clone = connection_cancellation_token.clone();
            let send_stream = send_stream.clone();

            tokio::spawn(async move {
                loop {
                    select! {
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {
                            let mut server_handle = send_stream.lock().await;
                            server_handle.write_all(&Message {uuid, msg_type: common_definitions::MessageType::KeepAlive}.into_sendable()).await.unwrap();
                        }
                        _ = connection_cancellation_token_clone.cancelled() => break,
                        recv_msg = pos_reciver.recv() => {
                            if let Some(msg) = recv_msg {
                                let mut server_handle = send_stream.lock().await;
                                server_handle.write_all(&Message {uuid, msg_type: msg}.into_sendable()).await.unwrap();
                            }
                        }
                    }
                }
            });

            pos_sender
        },
        message_reciver_from_server: {
            let (msg_sender, msg_reciver) = channel::<Message>(255);

            tokio::spawn(async move {
                loop {
                    select! {
                        _ = connection_cancellation_token.cancelled() => break,
                        mut recv_stream = recv_stream.lock() => {
                            let message_length = recv_stream.read_u64().await.unwrap();

                            if message_length > 128000000 {
                                println!("Incoming message length too large, refusing to acknowledge.");

                                continue;
                            }

                            let mut message_buf = vec![0; message_length as usize];

                            recv_stream.read_exact(&mut message_buf).await.unwrap();

                            msg_sender.send(Message::from_str(&String::from_utf8(message_buf).unwrap()).unwrap()).await.unwrap();
                        }
                    }
                }
            });

            msg_reciver
        },
    };

    Ok(session)
}

impl Default for Application {
    fn default() -> Self {
        let dock_state = DockState::new(vec![TabType::Canvas]);

        let mut open_tabs = HashSet::new();

        for node in dock_state[SurfaceIndex::main()].iter() {
            if let Some(tabs) = node.tabs() {
                for tab in tabs {
                    open_tabs.insert(*tab);
                }
            }
        }

        let context = ApplicationContext {
            open_tabs,
            ..Default::default()
        };

        Self {
            tree: dock_state,
            context,
            uuid: ClientIdentificator::default(),
        }
    }
}

/// This struct contains all the ```BrushTypes``` with their own width (```f32```) and color (```Color32```)
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PaintBrushes {
    /// The current ```BrushType```.
    brush_type: BrushType,
    /// The ```BrushType```-s' width.
    brush_width: [f32; BRUSH_TYPE_COUNT],
    /// The ```BrushType```-s' color.
    brush_color: [Color32; BRUSH_TYPE_COUNT],
}

impl Default for PaintBrushes {
    fn default() -> Self {
        Self {
            brush_type: BrushType::default(),
            brush_width: [1.0; BRUSH_TYPE_COUNT],
            brush_color: Default::default(),
        }
    }
}

impl PaintBrushes {
    /// Get current brush selected by the client.
    /// This function converts the ```BrushType``` to a usize as every ```BrushType``` has its own width and color.
    pub fn get_current_brush(&self) -> (f32, Color32, BrushType) {
        (
            self.brush_width[self.brush_type as usize],
            self.brush_color[self.brush_type as usize],
            self.brush_type,
        )
    }

    /// Get a mutable reference to the current brush and its properties.
    pub fn get_mut_current_brush(&mut self) -> (&mut f32, &mut Color32, &mut BrushType) {
        (
            &mut self.brush_width[self.brush_type as usize],
            &mut self.brush_color[self.brush_type as usize],
            &mut self.brush_type,
        )
    }

    /// Get the nth brush and its properties.
    pub fn get_nth_brush(&self, nth: usize) -> (f32, Color32, BrushType) {
        (
            self.brush_width[nth],
            self.brush_color[nth],
            self.brush_type,
        )
    }
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            let data = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();

            return data;
        }

        Self::default()
    }
}

/// This function takes ```&mut <T>``` and extension_filter: ```&str```, so a ```FileDialog``` can be opened.
/// This function reads the contents of the file specified by the FileDialog.
/// After reading the contents, it automaticly ```serde::Serialize```-es to type ```<T>```.
/// Then the ```&mut T``` is replaced with the new ```<T>``` handle.
fn read_file_into_memory<T: for<'a> Deserialize<'a>>(
    memory: &mut T,
    extension_filter: &str,
) -> anyhow::Result<()> {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Supported files", &[extension_filter])
        .pick_file()
    {
        let deserialized_context = fs::read(path)?;
        *memory = rmp_serde::from_slice::<T>(&miniz_oxide::inflate::decompress_to_vec(
            &deserialized_context,
        )?)?;
    }

    Ok(())
}

/// Displays an error ```MessageBox```
fn display_error(err: impl ToString) {
    rfd::MessageDialog::new()
        .set_title("Error")
        .set_description(err.to_string())
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
