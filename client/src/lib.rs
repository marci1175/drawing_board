use egui::{Color32, Pos2, Stroke};
use strum::{EnumCount, IntoStaticStr};
mod app;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct Application {
    lines: Vec<(Vec<Pos2>, (f32, Color32))>,
    paintbrush: PaintBrush,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub struct PaintBrush {
    brush_type: BrushType,
    brush_width: [f32; BrushType::COUNT],
    brush_color: [Color32; BrushType::COUNT],
}

impl PaintBrush {
    pub fn get_current_brush(&self) -> (f32, Color32)
    {
        (self.brush_width[self.brush_type as usize], self.brush_color[self.brush_type as usize])
    }

    pub fn get_mut_current_brush(&mut self) -> (&mut f32, &mut Color32) {
        (&mut self.brush_width[self.brush_type as usize], &mut self.brush_color[self.brush_type as usize])
    }

    pub fn get_nth_brush(&self, nth: usize) -> (f32, Color32)
    {
        (self.brush_width[nth], self.brush_color[nth])
    }
}


#[derive(serde::Serialize, serde::Deserialize, Default, PartialEq, Clone, Copy, EnumCount, IntoStaticStr, Debug)]
pub enum BrushType {
    Graffiti,
    Pencil,
    #[default]
    Marker,
    Eraser,
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            let data: Application =
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        
            return data;
        }

        Self::default()
    }
}