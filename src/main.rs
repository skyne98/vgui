use eframe::egui;
use eyre::{Context, Result};

fn main() -> Result<()> {
    color_eyre::install()?;

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "vgui demo",
        native_options,
        Box::new(|cc| Box::new(MyEguiApp::new(cc))),
    )
    .map_err(|e| eyre::eyre!(format!("{:?}", e)))
    .wrap_err("Failed to run eframe")?;

    Ok(())
}

#[derive(Default)]
struct MyEguiApp {}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
        });
    }
}
