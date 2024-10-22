use egui::{emath, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui};

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct Application {
    lines: Vec<Vec<Pos2>>,
    stroke: Stroke,
}

enum Brush {
    Grafiti,
    Pencil,
    Marker,
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

    pub fn ui_content(&mut self, ui: &mut Ui) -> egui::Response {
        let (mut response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::drag());

        let to_screen = emath::RectTransform::from_to(
            Rect::from_min_size(Pos2::ZERO, response.rect.square_proportions()),
            response.rect,
        );

        let from_screen = to_screen.inverse();

        if self.lines.is_empty() {
            self.lines.push(vec![]);
        }

        let current_line = self.lines.last_mut().unwrap();

        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let canvas_pos = from_screen * pointer_pos;
            if current_line.last() != Some(&canvas_pos) {
                current_line.push(canvas_pos);
                response.mark_changed();
            }
        } else if !current_line.is_empty() {
            self.lines.push(vec![]);
            response.mark_changed();
        }

        let shapes = self
            .lines
            .iter()
            .filter(|line| line.len() >= 2)
            .map(|line| {
                let points: Vec<Pos2> = line.iter().map(|p| to_screen * *p).collect();
                egui::Shape::line(points, self.stroke)
            });

        painter.extend(shapes);

        response
    }
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("settings_bar").show(ctx, |ui| {
            ui.allocate_space(vec2(ui.available_width(), 10.));
            
            ui.horizontal(|ui| {
                ui.menu_button("Brush", |ui| {
                    // ui.selectable_value(&Brush::Marker, selected_value, text)
                });

                let mut color: [u8; 4] = self.stroke.color.to_array();

                ui.color_edit_button_srgba_premultiplied(&mut color);

                self.stroke.color = Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
            });

            ui.allocate_space(vec2(ui.available_width(), 10.));
        });
        
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| self.ui_content(ui));
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}