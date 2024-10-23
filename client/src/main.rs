use std::sync::Arc;

use drawing_board_client::Application;
use eframe::NativeOptions;
use egui::{ColorImage, ImageData, TextureOptions};

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        ..Default::default()
    };

    eframe::run_native(
        "Draw",
        native_options,
        Box::new(|cc| {
            let application = Application::new(cc);
            
            // cc.egui_ctx.load_texture("_paint_graffiti", ImageData::Color(Arc::new(ColorImage::example())), TextureOptions::LINEAR_REPEAT);

            Ok(Box::new(application))
        }),
    )
}
