use egui::{
    ahash::{HashSet, HashSetExt},
    util::undoer::Undoer,
    Color32, Pos2,
};
use egui_dock::{DockState, SurfaceIndex};
use strum::{EnumCount, IntoStaticStr};
mod app;

pub type BrushMap = Vec<(Vec<Pos2>, (f32, Color32, BrushType))>;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ApplicationContext {
    lines: BrushMap,
    paintbrush: PaintBrush,

    undoer: Undoer<BrushMap>,
    open_tabs: HashSet<TabType>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Application {
    tree: DockState<TabType>,
    context: ApplicationContext,
}
#[derive(
    IntoStaticStr, Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Hash, PartialEq, Eq,
)]
pub enum TabType {
    Canvas,
    BrushSettings,
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
