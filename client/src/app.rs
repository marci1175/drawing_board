use crate::{Application, BrushType};
use egui::{
    emath::{self, Rot2}, frame, vec2, Color32, Mesh, Pos2, Rect, Sense, Stroke, Ui
};

impl Application {
    pub fn ui_content(&mut self, ui: &mut Ui) -> egui::Response {
        let (mut response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::drag());

        let paint_area_square_proportions = response.rect.square_proportions();

        let to_screen = emath::RectTransform::from_to(
            Rect::from_min_size(Pos2::ZERO, paint_area_square_proportions),
            response.rect,
        );

        let from_screen = to_screen.inverse();

        match self.paintbrush.brush_type {
            BrushType::Graffiti | BrushType::Pencil | BrushType::Marker => {
                if self.lines.is_empty() {
                    self.lines.push((
                        vec![],
                        self.paintbrush
                            .get_nth_brush(self.paintbrush.brush_type as usize),
                    ));
                }
        
                let current_line = self.lines.last_mut().unwrap();
        
                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    let canvas_pos = from_screen * pointer_pos;
                    if current_line.0.last() != Some(&canvas_pos) {
                        current_line.0.push(canvas_pos);
                        current_line.1 = self
                            .paintbrush
                            .get_nth_brush(self.paintbrush.brush_type as usize);
                        response.mark_changed();
                    }

                } else if !current_line.0.is_empty() {
                    self.lines.push((
                        vec![],
                        self.paintbrush
                            .get_nth_brush(self.paintbrush.brush_type as usize),
                    ));
                    response.mark_changed();
                    self.undoer.add_undo(&self.lines);
                }
        
                let shapes = self
                    .lines
                    .iter()
                    .filter(|line| line.0.len() >= 2)
                    .map(|line| draw_line_to_screen_with_brush(line, to_screen));
        
                painter.extend(shapes);
            },
            BrushType::Eraser => {
                    if let Some(pointer_pos) = response.interact_pointer_pos() {
                    let (brush_width, _, _) = self.paintbrush.get_current_brush();
                    for (line_idx, (lines_pos, (line_width, _, _))) in self.lines.clone().iter().enumerate() {
                        let mut last_rect = Rect::NOTHING;

                        for line_pos in lines_pos {
                            let current_rect = Rect::from_center_size(to_screen * *line_pos, vec2(*line_width + brush_width, *line_width + brush_width));
                            let rect = last_rect.union(current_rect);

                            if rect.contains(pointer_pos) {
                                self.lines.remove(line_idx);
                                self.undoer.add_undo(&self.lines);
                                response.mark_changed();
                                break;
                            }

                            last_rect = current_rect;
                        }
                    }
                }

                painter.extend(self.lines.iter().map(|line| draw_line_to_screen_with_brush(line, to_screen)));
            }
            BrushType::None => {

            }
        }


        response
    }

    fn color_picker(&mut self, ui: &mut Ui) {
        let mut color: [u8; 4] = self.paintbrush.get_current_brush().1.to_array();

        ui.color_edit_button_srgba_premultiplied(&mut color);

        *self.paintbrush.get_mut_current_brush().1 =
            Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
    }
}

fn draw_line_to_screen_with_brush(
    line: &(Vec<Pos2>, (f32, Color32, BrushType)),
    to_screen: emath::RectTransform,
) -> egui::Shape {
    let points: Vec<Pos2> = line.0.iter().map(|p| to_screen * *p).collect();
    let (width, color, brush_type) = line.1;

    match brush_type {
        BrushType::Pencil => egui::Shape::Vec(egui::Shape::dashed_line(
            &points,
            Stroke::new(width, color),
            width,
            width,
        )),
        BrushType::Marker => egui::Shape::line(points, Stroke::new(width, color)),
        BrushType::Graffiti => egui::Shape::Noop,
        BrushType::Eraser => egui::Shape::Noop,
        BrushType::None => egui::Shape::Noop,
    }
}

fn draw_line_with_brush(line: &(Vec<Pos2>, (f32, Color32, BrushType))) -> egui::Shape {
    let (width, color, brush_type) = line.1;

    match brush_type {
        BrushType::Pencil => egui::Shape::Vec(egui::Shape::dashed_line(
            &line.0,
            Stroke::new(width, color),
            width,
            width,
        )),
        BrushType::Marker => egui::Shape::line(line.0.clone(), Stroke::new(width, color)),
        BrushType::Eraser => egui::Shape::Noop,
        BrushType::None => egui::Shape::Noop,
        BrushType::Graffiti => egui::Shape::Noop,
    }
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("settings_bar").show(ctx, |ui| {
            ui.allocate_space(vec2(ui.available_width(), 10.));

            ui.horizontal(|ui| {
                let paintbrush_name: &'static str = self.paintbrush.brush_type.into();
                ui.menu_button(paintbrush_name, |ui| {
                    ui.selectable_value(
                        &mut self.paintbrush.brush_type,
                        BrushType::Marker,
                        "Marker",
                    );
                    ui.selectable_value(
                        &mut self.paintbrush.brush_type,
                        BrushType::Graffiti,
                        "Graffiti",
                    );
                    ui.selectable_value(
                        &mut self.paintbrush.brush_type,
                        BrushType::Pencil,
                        "Pencil",
                    );
                    ui.selectable_value(
                        &mut self.paintbrush.brush_type,
                        BrushType::Eraser,
                        "Eraser",
                    );
                });

                self.color_picker(ui);

                ui.add(
                    egui::Slider::new(
                        &mut self.paintbrush.brush_width[self.paintbrush.brush_type as usize],
                        1.0..=100.0,
                    )
                    .step_by(0.2),
                );

                let (_, allocated_rect) = ui.allocate_space(vec2(50., ui.available_height()));

                ui.painter_at(allocated_rect).add(draw_line_with_brush(&(
                    vec![allocated_rect.left_center(), allocated_rect.right_center()],
                    self.paintbrush.get_current_brush(),
                )));

                let can_undo = self.undoer.has_undo(&self.lines);
                let can_redo = self.undoer.has_redo(&self.lines);

                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo"))
                    .clicked()
                {
                    if let Some(state) = self.undoer.undo(&self.lines) {
                        self.lines = state.clone();
                    }
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo"))
                    .clicked()
                {
                    if let Some(state) = self.undoer.redo(&self.lines) {
                        self.lines = state.clone();
                    }
                }

                if ui.button("Erase board").clicked() {
                    self.lines.clear();
                }
            });

            ui.allocate_space(vec2(ui.available_width(), 10.));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| self.ui_content(ui));

            if let Some(pointer_pos) = ctx.pointer_hover_pos() {
                let (size, color, _) = self.paintbrush.get_current_brush();
                ui.painter().circle_filled(pointer_pos, size / 2., color.gamma_multiply(0.5));
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
