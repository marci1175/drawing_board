use drawing_board_client::Application;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        ..Default::default()
    };

    eframe::run_native(
        "Draw",
        native_options,
        Box::new(|cc| Ok(Box::new(Application::new(cc)))),
    )
}
