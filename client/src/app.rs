use std::{collections::HashMap, fs, sync::mpsc};

use crate::{
    connect_to_server, display_error, read_file_into_memory, Application, ApplicationContext,
    ConnectionSession, FileSession, TabType, DRAWING_BOARD_IMAGE_EXT, DRAWING_BOARD_WORKSPACE_EXT,
};
use common_definitions::{Brush, BrushType, LinePos, PointerProperties};
use egui::{
    emath::{self},
    vec2, Align2, CentralPanel, Color32, Context, FontId, Frame, Key, Modifiers, Pos2, Rect,
    RichText, Sense, Stroke, TopBottomPanel, Ui, Vec2,
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
                if self.paintbrush.get_current_brush().1.a() != 0 {
                    if self.lines.is_empty() {
                        self.lines.push((
                            vec![],
                            self.paintbrush
                                .get_nth_brush(self.paintbrush.brush_type as usize),
                        ));
                    }

                    let last_line_entry = self.lines.last_mut().unwrap();
                    if let Some(pointer_pos) = response.interact_pointer_pos() {
                        let on_canvas_pointer_pos = from_screen * pointer_pos;
                        if last_line_entry.0.last() != Some(&on_canvas_pointer_pos.into()) {
                            last_line_entry.0.push(on_canvas_pointer_pos.into());
                            last_line_entry.1 = self
                                .paintbrush
                                .get_nth_brush(self.paintbrush.brush_type as usize);

                            response.mark_changed();
                        }
                    } else if !last_line_entry.0.is_empty() {
                        if let Some(current_session) = &self.connection.current_session {
                            let current_line = self.lines.last().unwrap();
                            if let Err(err) = current_session.sender_to_server.try_send(
                                common_definitions::MessageType::AddLine((
                                    current_line.0.to_vec(),
                                    current_line.1,
                                )),
                            ) {
                                dbg!(err);

                                self.connection.current_session = None;
                            }
                        }

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
                }
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
                                to_screen * (*line_pos).into(),
                                vec2(*line_width + brush_width, *line_width + brush_width),
                            );
                            let rect = last_rect.union(current_rect);

                            if rect.contains(pointer_pos) {
                                self.lines.swap_remove(line_idx);

                                self.undoer.add_undo(&self.lines);

                                response.mark_changed();
                                break;
                            }

                            last_rect = current_rect;
                        }
                    }
                }
            }
            BrushType::None => {
                for (line_pos, _) in self.lines.iter() {
                    let line_rect = Rect::from_points(
                        &line_pos
                            .iter()
                            .map(|pos| to_screen * Into::<Pos2>::into(*pos))
                            .collect::<Vec<Pos2>>(),
                    );

                    if let Some(pointer_pos) = ui.ctx().pointer_hover_pos() {
                        if line_rect.contains(pointer_pos) {
                            ui.painter().rect(
                                line_rect,
                                1.,
                                Color32::from_rgba_unmultiplied(0, 255, 0, 80),
                                Stroke::new(2., Color32::GREEN),
                            );

                            //If It matched just continue with the for loop so that this line's rect wont get displayed twice
                            continue;
                        }
                    }

                    ui.painter().rect(
                        line_rect,
                        1.,
                        Color32::TRANSPARENT,
                        Stroke::new(1., Color32::WHITE),
                    );
                }
            }
        }

        painter.extend(
            self.lines
                .iter()
                .filter(|line| line.0.len() >= 2)
                .map(|line| draw_line_to_screen_with_brush(line, to_screen)),
        );

        response
    }

    /// This function handles the usage of a colorpicker for multiple paintbrushes.
    fn color_picker(&mut self, ui: &mut Ui) {
        let mut color: [u8; 4] = self.paintbrush.get_current_brush().1.to_array();

        ui.color_edit_button_srgba_premultiplied(&mut color);

        *self.paintbrush.get_mut_current_brush().1 =
            Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
    }
}

/// This function draws a line ((Vec<LinePos>, Brush)) to the screen.
fn draw_line_to_screen_with_brush(
    line: &(Vec<LinePos>, Brush),
    to_screen: emath::RectTransform,
) -> egui::Shape {
    let points: Vec<Pos2> = line.0.iter().map(|p| to_screen * (*p).into()).collect();
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
                        ui.selectable_value(
                            &mut self.paintbrush.brush_type,
                            BrushType::None,
                            "None",
                        );
                    });
                });

                ui.separator();

                //The `BrushType`-s which properties cannot be changed
                match (
                    matches!(self.paintbrush.get_current_brush().2, BrushType::None),
                    matches!(self.paintbrush.get_current_brush().2, BrushType::Eraser),
                ) {
                    (false, true) => {
                        ui.label("Width");
                        ui.add(
                            egui::Slider::new(
                                &mut self.paintbrush.brush_width
                                    [self.paintbrush.brush_type as usize],
                                1.0..=100.0,
                            )
                            .step_by(0.2),
                        );
                    }
                    (false, false) => {
                        ui.horizontal(|ui| {
                            ui.label("Color");
                            self.color_picker(ui);
                        });

                        ui.label("Width");
                        ui.add(
                            egui::Slider::new(
                                &mut self.paintbrush.brush_width
                                    [self.paintbrush.brush_type as usize],
                                1.0..=100.0,
                            )
                            .step_by(0.2),
                        );
                    }

                    _ => (),
                }

                ui.separator();

                let can_undo = self.undoer.has_undo(&self.lines);
                let can_redo = self.undoer.has_redo(&self.lines);

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo"))
                        .clicked()
                        || ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Z))
                    {
                        if let Some(state) = self.undoer.undo(&self.lines) {
                            self.lines = state.clone();
                        }
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo"))
                        .clicked()
                        || ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Y))
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
                    ui.add_enabled_ui(self.context.connection.current_session.is_none(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Target Address");
                            ui.text_edit_singleline(&mut self.context.connection.target_address);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Username");
                            ui.text_edit_singleline(&mut self.context.connection.username);
                        });
                    });

                    if let Some(connection_session) = &self.context.connection.current_session {
                        if let Ok(current_session) = connection_session
                            .connection_handle
                            .try_read()
                            .map(|con| con.clone())
                        {
                            let ping = current_session.rtt().as_millis();
                            let clamped_ping = ping.clamp(0, 255) as u8;
                            ui.label(
                                RichText::new(format!("Estimated ping: {ping}ms"))
                                    .color(Color32::from_rgb(clamped_ping, 255 - clamped_ping, 0)),
                            );

                            if ui.button("Disconnect").clicked() {
                                //Reset connection state
                                connection_session.cancel_connection();
                                self.context.lines.clear();
                                self.context.connection.connected_clients.clear();
                                self.context.connection.session_reciver = None;
                                self.context.connection.current_session = None;
                            }
                        }
                    } else if ui.button("Connect").clicked() {
                        let (sender, reciver) = mpsc::channel::<ConnectionSession>();
                        let target_address = self.context.connection.target_address.clone();
                        let username = self.context.connection.username.clone();
                        let uuid = self.uuid.0;

                        self.context.connection.session_reciver = Some(reciver);

                        let ctx_clone = ctx.clone();

                        tokio::spawn(async move {
                            match connect_to_server(target_address, username, dbg!(uuid)).await {
                                Ok(session) => {
                                    ctx_clone.request_repaint();
                                    sender.send(session).unwrap();
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

                for (_, (username, pointer_properties)) in
                    self.context.connection.connected_clients.iter()
                {
                    let cursor_pos = pointer_properties.pointer_pos;

                    ui.painter().circle(
                        cursor_pos,
                        15.,
                        Color32::WHITE,
                        Stroke::new(1., Color32::WHITE),
                    );
                    ui.painter().text(
                        cursor_pos + Vec2::new(50., 30.),
                        Align2::CENTER_CENTER,
                        username,
                        FontId::monospace(20.),
                        Color32::GRAY,
                    );

                    ctx.request_repaint();
                }
            });

        if let Some(reciver) = &self.context.connection.session_reciver {
            if let Ok(val) = reciver.try_recv() {
                self.context.connection.current_session = Some(val);

                //Clear lines on successful connection
                self.context.lines.clear();
            }
        }

        if let Some(session) = self.context.connection.current_session.as_mut() {
            while let Ok(message) = session.message_reciver_from_server.try_recv() {
                match message.msg_type {
                    common_definitions::MessageType::ClientList(clients) => {
                        self.context.connection.connected_clients =
                            HashMap::from_iter(clients.iter().map(|entry| {
                                (entry.1, (entry.0.clone(), PointerProperties::default()))
                            }))
                    }
                    common_definitions::MessageType::CursorPosition(client_pos) => {
                        if let Some((_, pos)) = self
                            .context
                            .connection
                            .connected_clients
                            .get_mut(&message.uuid)
                        {
                            *pos = client_pos;
                        }
                    }
                    common_definitions::MessageType::Connecting(username) => {
                        self.context
                            .connection
                            .connected_clients
                            .insert(message.uuid, (username, PointerProperties::default()));
                    }
                    common_definitions::MessageType::Disconnecting => {
                        self.context
                            .connection
                            .connected_clients
                            .remove(&message.uuid);
                    }

                    //Acknowledge keepalive message
                    common_definitions::MessageType::KeepAlive => (),
                    common_definitions::MessageType::AddLine(line_data) => {
                        if !self.context.lines.iter().any(|line| *line == line_data) {
                            self.context.lines.push((line_data.0, line_data.1));

                            self.context.lines.push((
                                vec![],
                                self.context
                                    .paintbrush
                                    .get_nth_brush(self.context.paintbrush.brush_type as usize),
                            ));
                        }
                    }
                    common_definitions::MessageType::ModifyLine((pos, props)) => {
                        if let Some(idx) = self
                            .context
                            .lines
                            .clone()
                            .iter()
                            .position(|line| *line.0 == pos)
                        {
                            if let Some(line_modification) = props {
                                self.context.lines[idx].1 = line_modification;
                            } else {
                                self.context.lines.swap_remove(idx);
                            }
                        } else {
                            session
                                .sender_to_server
                                .try_send(common_definitions::MessageType::RequestSyncLine(Some(
                                    pos,
                                )))
                                .unwrap();
                        }
                    }
                    common_definitions::MessageType::RequestSyncLine(_) => {
                        unimplemented!("The server wont send client messages.")
                    }
                    common_definitions::MessageType::SyncLine(line_sync_type) => {
                        match line_sync_type {
                            common_definitions::LineSyncType::Full(server_lines) => {
                                self.context.lines = server_lines
                            }
                            common_definitions::LineSyncType::Partial(line) => match line {
                                Some(line) => {
                                    self.context.lines.push((line.0, line.1));
                                }
                                None => {
                                    eprintln!("Server/Client desync, another client requested the modification of a line that doesn't exist on the server's side.");
                                }
                            },
                        }
                    }
                };
            }

            if let Some(cur_pos) = ctx.pointer_latest_pos() {
                if let Err(err) = session.sender_to_server.try_send(
                    common_definitions::MessageType::CursorPosition(PointerProperties {
                        pointer_pos: cur_pos,
                        brush: self.context.paintbrush.get_current_brush(),
                    }),
                ) {
                    dbg!(err);

                    self.context.connection.current_session = None;
                }
            }
        };
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl Application {
    /// This function creates a new ```FileSession``` if a file is saved as or opened.
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
