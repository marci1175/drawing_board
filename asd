egui::TopBottomPanel::top("settings_bar").show(ctx, |ui| {
            ui.allocate_space(vec2(ui.available_width(), 10.));

            ui.columns_const(|[col_1, col_2]| {
                col_1.horizontal(|ui| {
                    ui.menu_button("Brush", |ui| {
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
                                &mut self.paintbrush.brush_width
                                    [self.paintbrush.brush_type as usize],
                                1.0..=100.0,
                            )
                            .step_by(0.2),
                        );

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Preview");
                            let (_, allocated_rect) = ui.allocate_space(ui.available_size());

                            ui.painter_at(allocated_rect).add(draw_line_with_brush(&(
                                vec![allocated_rect.left_center(), allocated_rect.right_center()],
                                self.paintbrush.get_current_brush(),
                            )));
                        });

                    });

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

                col_2.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.button("Connect");
                });
            });
            ui.allocate_space(vec2(ui.available_width(), 10.));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| self.ui_content(ui));

            if let Some(pointer_pos) = ctx.pointer_hover_pos() {
                let (size, color, _) = self.paintbrush.get_current_brush();
                ui.painter()
                    .circle_filled(pointer_pos, size / 2., color.gamma_multiply(0.5));
            }
        });