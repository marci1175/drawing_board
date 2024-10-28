use std::fs;

use crate::{
    connect_to_server, display_error, read_file_into_memory, Application, ApplicationContext,
    BrushType, FileSession, TabType, DRAWING_BOARD_IMAGE_EXT, DRAWING_BOARD_WORKSPACE_EXT,
};
use egui::{
    emath::{self},
    vec2, CentralPanel, Color32, Context, Frame, Pos2, Rect, RichText, Sense, Stroke,
    TopBottomPanel, Ui,
};
use egui_dock::{DockArea, TabViewer};

impl ApplicationContext {
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

                    if !self.undoer.is_in_flux() {
                        self.undoer.add_undo(&self.lines);
                    }

                    response.mark_changed();
                }

                let shapes = self
                    .lines
                    .iter()
                    .filter(|line| line.0.len() >= 2)
                    .map(|line| draw_line_to_screen_with_brush(line, to_screen));

                painter.extend(shapes);
            }
            BrushType::Eraser => {
                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    let (brush_width, _, _) = self.paintbrush.get_current_brush();
                    for (line_idx, (lines_pos, (line_width, _, _))) in
                        self.lines.clone().iter().enumerate()
                    {
                        let mut last_rect = Rect::NOTHING;

                        for line_pos in lines_pos {
                            let current_rect = Rect::from_center_size(
                                to_screen * *line_pos,
                                vec2(*line_width + brush_width, *line_width + brush_width),
                            );
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

                painter.extend(
                    self.lines
                        .iter()
                        .map(|line| draw_line_to_screen_with_brush(line, to_screen)),
                );
            }
            BrushType::None => {}
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
fn _draw_line_with_brush(line: &(Vec<Pos2>, (f32, Color32, BrushType))) -> egui::Shape {
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

impl TabViewer for ApplicationContext {
    type Tab = TabType;
    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let widget_title: &'static str = (*tab).into();

        widget_title.into()
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        match _tab {
            TabType::Canvas => false,
            TabType::BrushSettings => true,
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            TabType::Canvas => {
                let canvas = egui::Frame::canvas(ui.style()).show(ui, |ui| self.ui_content(ui));

                if let Some(pointer_pos) = ui.ctx().pointer_hover_pos() {
                    let (size, color, _) = self.paintbrush.get_current_brush();
                    ui.painter()
                        .circle_filled(pointer_pos, size / 2., color.gamma_multiply(0.5));
                }

                if let Some(export_save_path) = &self.export_path {
                    ui.ctx().input(|i| {
                        for event in &i.raw.events {
                            if let egui::Event::Screenshot { image, .. } = event {
                                let pixels_per_point = i.pixels_per_point();
                                let region = canvas.inner.rect;
                                let image_region = image.region(&region, Some(pixels_per_point));
                                if let Err(err) = image::save_buffer(
                                    export_save_path,
                                    image_region.as_raw(),
                                    image_region.width() as u32,
                                    image_region.height() as u32,
                                    image::ColorType::Rgba8,
                                ) {
                                    display_error(err);
                                }
                            }
                        }
                    });
                }
            }
            TabType::BrushSettings => {
                ui.allocate_space(vec2(ui.available_width(), 10.));

                let paintbrush_name: &'static str = self.paintbrush.brush_type.into();
                ui.horizontal(|ui| {
                    ui.label("Brush type");
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
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Color");
                    self.color_picker(ui);
                });

                ui.label("Width");
                ui.add(
                    egui::Slider::new(
                        &mut self.paintbrush.brush_width[self.paintbrush.brush_type as usize],
                        1.0..=100.0,
                    )
                    .step_by(0.2),
                );

                ui.separator();

                let can_undo = self.undoer.has_undo(&self.lines);
                let can_redo = self.undoer.has_redo(&self.lines);

                ui.horizontal(|ui| {
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
            }
        }
    }
}
impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        TopBottomPanel::top("settings_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New File").clicked() {
                        self.reset();
                    }

                    if ui.button("Open Image").clicked() {
                        if let Err(err) =
                            read_file_into_memory(&mut self.context.lines, DRAWING_BOARD_IMAGE_EXT)
                        {
                            display_error(err);
                        };
                    }

                    if ui.button("Open Workspace").clicked() {
                        if let Err(err) =
                            read_file_into_memory(&mut self.context, DRAWING_BOARD_WORKSPACE_EXT)
                        {
                            display_error(err);
                        };
                    }

                    ui.separator();

                    if ui.button("Save Workspace As").clicked() {
                        if let Some(saved_file_path) = rfd::FileDialog::new()
                            .add_filter("Project File", &[DRAWING_BOARD_WORKSPACE_EXT])
                            .save_file()
                        {
                            if let Err(err) = fs::write(
                                saved_file_path,
                                miniz_oxide::deflate::compress_to_vec(
                                    &rmp_serde::to_vec(&self.context).unwrap(),
                                    10,
                                ),
                            ) {
                                display_error(err);
                            }
                        };
                    }

                    if ui.button("Save Image").clicked() {
                        let compressed_save_file = miniz_oxide::deflate::compress_to_vec(
                            &rmp_serde::to_vec(&self.context).unwrap(),
                            10,
                        );

                        if let Some(session) = &self.context.file_session {
                            if let Err(err) =
                                fs::write(session.file_path.clone(), compressed_save_file)
                            {
                                display_error(err);
                            }
                        } else if let Some(saved_file_path) = rfd::FileDialog::new()
                            .add_filter("Project File", &[DRAWING_BOARD_WORKSPACE_EXT])
                            .save_file()
                        {
                            if let Err(err) = fs::write(saved_file_path, compressed_save_file) {
                                display_error(err);
                            }
                        }
                    }

                    if ui.button("Save Image As").clicked() {
                        if let Some(saved_file_path) = rfd::FileDialog::new()
                            .add_filter("Project File", &[DRAWING_BOARD_IMAGE_EXT])
                            .save_file()
                        {
                            if let Err(err) = fs::write(
                                &saved_file_path,
                                miniz_oxide::deflate::compress_to_vec(
                                    &rmp_serde::to_vec(&self.context.lines).unwrap(),
                                    10,
                                ),
                            ) {
                                display_error(err);
                            }

                            self.create_session(saved_file_path, ctx);
                        };
                    }

                    if ui.button("Export As Png").clicked() {
                        if let Some(save_path) = rfd::FileDialog::new()
                            .add_filter("Image", &["png"])
                            .save_file()
                        {
                            self.context.export_path = Some(save_path);
                            ui.close_menu();

                            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                        }
                    }

                    ui.separator();

                    ui.checkbox(&mut true, "Auto Save");

                    ui.separator();

                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Workspace", |ui| {
                    ui.menu_button("Tooling", |ui| {
                        let tree_node = self.tree.find_tab(&TabType::BrushSettings);
                        if ui
                            .checkbox(&mut tree_node.is_some(), "Brush settings")
                            .clicked()
                        {
                            if let Some(node) = tree_node {
                                self.tree.remove_tab(node);
                            } else {
                                self.tree.push_to_focused_leaf(TabType::BrushSettings);
                            }
                        };
                    });
                });

                ui.menu_button("Connections", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Target Address");
                        ui.add_enabled_ui(
                            self.context
                                .connection
                                .current_session
                                .connection
                                .try_read()
                                .is_ok_and(|inner| inner.is_none()),
                            |ui| {
                                ui.text_edit_singleline(
                                    &mut self.context.connection.target_address,
                                );
                            },
                        );
                    });

                    if let Ok(Some(current_session)) = &self
                        .context
                        .connection
                        .current_session
                        .connection
                        .try_read()
                        .map(|con| con.clone())
                    {
                        let ping = current_session.rtt().as_millis();
                        let clamped_ping = ping.clamp(0, 255) as u8;
                        ui.label(
                            RichText::new(format!("Estimated ping: {ping}ms"))
                                .color(Color32::from_rgb(clamped_ping, 255 - clamped_ping, 0)),
                        );
                        if ui.button("Disconnect").clicked() {}
                    } else if ui.button("Connect").clicked() {
                        let target_address = self.context.connection.target_address.clone();
                        let current_connection =
                            self.context.connection.current_session.connection.clone();

                        tokio::spawn(async move {
                            match connect_to_server(target_address).await {
                                Ok(client) => {
                                    *current_connection.write().await = Some(client);
                                }
                                Err(err) => {
                                    display_error(err);
                                }
                            }
                        });
                    }
                });
            });
        });

        CentralPanel::default()
            .frame(Frame::central_panel(&ctx.style()).inner_margin(0.))
            .show(ctx, |ui| {
                DockArea::new(&mut self.tree)
                    .show_window_close_buttons(true)
                    .show_inside(ui, &mut self.context);
            });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl Application {
    fn create_session(&mut self, saved_file_path: std::path::PathBuf, ctx: &Context) {
        let project_name = saved_file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        self.context.file_session = Some(FileSession::create_session(
            saved_file_path.clone(),
            project_name.clone(),
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(project_name));
    }
}
