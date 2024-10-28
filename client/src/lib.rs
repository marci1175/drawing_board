pub const DRAWING_BOARD_IMAGE_EXT: &str = "dbimg";
pub const DRAWING_BOARD_WORKSPACE_EXT: &str = "dbproject";

use chrono::{Local, NaiveDate};
use egui::{
    ahash::{HashSet, HashSetExt},
    util::undoer::Undoer,
    Color32, ColorImage, Pos2,
};
use egui_dock::{DockState, SurfaceIndex};
use quinn::{
    crypto::rustls::QuicClientConfig,
    rustls::{
        self,
        pki_types::{CertificateDer, ServerName, UnixTime},
    },
    ClientConfig, Connection, Endpoint,
};
use serde::Deserialize;
use std::{fs, net::Ipv6Addr, path::PathBuf, sync::Arc};
use strum::{EnumCount, IntoStaticStr};
use tokio::sync::RwLock;
mod app;

pub type BrushMap = Vec<(Vec<Pos2>, (f32, Color32, BrushType))>;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ApplicationContext {
    lines: BrushMap,
    paintbrush: PaintBrush,

    file_session: Option<FileSession>,

    undoer: Undoer<BrushMap>,
    open_tabs: HashSet<TabType>,

    connection: ConnectionData,

    export_path: Option<PathBuf>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ConnectionData {
    target_address: String,

    #[serde(skip)]
    current_session: ConnectionSession,
}

#[derive(serde::Deserialize, Default)]
pub struct ConnectionSession {
    #[serde(skip)]
    connection: Arc<RwLock<Option<Connection>>>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct FileSession {
    pub file_path: PathBuf,
    pub project_name: String,
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

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Application {
    tree: DockState<TabType>,
    context: ApplicationContext,
    image: Arc<ColorImage>,
}

impl Application {
    pub fn reset(&mut self) {
        *self = Application::default();
    }
}

#[derive(
    IntoStaticStr, Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Hash, PartialEq, Eq,
)]
pub enum TabType {
    Canvas,
    BrushSettings,
}

#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

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

pub async fn connect_to_server(target_address: String) -> anyhow::Result<quinn::Connection> {
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

    Ok(client)
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
            image: Arc::new(ColorImage::default()),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PaintBrush {
    brush_type: BrushType,
    brush_width: [f32; BrushType::COUNT],
    brush_color: [Color32; BrushType::COUNT],
}

impl Default for PaintBrush {
    fn default() -> Self {
        Self {
            brush_type: BrushType::default(),
            brush_width: [1.0; BrushType::COUNT],
            brush_color: Default::default(),
        }
    }
}

impl PaintBrush {
    pub fn get_current_brush(&self) -> (f32, Color32, BrushType) {
        (
            self.brush_width[self.brush_type as usize],
            self.brush_color[self.brush_type as usize],
            self.brush_type,
        )
    }

    pub fn get_mut_current_brush(&mut self) -> (&mut f32, &mut Color32, &mut BrushType) {
        (
            &mut self.brush_width[self.brush_type as usize],
            &mut self.brush_color[self.brush_type as usize],
            &mut self.brush_type,
        )
    }

    pub fn get_nth_brush(&self, nth: usize) -> (f32, Color32, BrushType) {
        (
            self.brush_width[nth],
            self.brush_color[nth],
            self.brush_type,
        )
    }
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Default,
    PartialEq,
    Clone,
    Copy,
    EnumCount,
    IntoStaticStr,
    Debug,
)]
pub enum BrushType {
    None,
    Graffiti,
    Pencil,
    #[default]
    Marker,
    Eraser,
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
fn display_error(err: impl ToString) {
    rfd::MessageDialog::new()
        .set_title("Error")
        .set_description(err.to_string())
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
