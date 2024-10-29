use drawing_board_client::Application;
use eframe::NativeOptions;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    /* TODO:
        Add textures for painting
        Create a voice call library
        Create the ability to have more boards at once
    */

    #[cfg(debug_assertions)]
    console_subscriber::init();

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
