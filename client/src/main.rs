use drawing_board_client::Application;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    /* TODO:
        Add textures for painting
        Create a voice call library
        Start writing server
        Create the ability to save the boards
        Create the ability to have more boards at once
    */
    
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
