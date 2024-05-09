use boa_engine::{context::ContextBuilder, js_string, property::Attribute, Source};
use boa_runtime::Console;
use eframe::egui;
use eyre::{Context, Result};

fn main() -> Result<()> {
    color_eyre::install()?;

    // Set up the JS virtual machine
    let vue_code = include_str!("../assets/vue.global.js");

    // Instantiate the execution context
    let mut context = ContextBuilder::default()
        .build()
        .expect("Building the default context should not fail");

    // Add the `console.log` function to the context
    let console = Console::init(&mut context);
    context
        .register_global_property(js_string!(Console::NAME), console, Attribute::all())
        .expect("the console object shouldn't exist");

    // Parse the source code
    match context.eval(Source::from_bytes(vue_code)) {
        Ok(res) => {
            println!(
                "{}",
                res.to_string(&mut context).unwrap().to_std_string_escaped()
            );
        }
        Err(e) => {
            // Pretty print the error
            eprintln!("Uncaught {e}");
        }
    };

    // Try to initialize the Vue app
    match context.eval(Source::from_bytes("Vue.createApp({}).mount('#app')")) {
        Ok(res) => {
            println!(
                "{}",
                res.to_string(&mut context).unwrap().to_std_string_escaped()
            );
        }
        Err(e) => {
            // Pretty print the error
            eprintln!("Uncaught {e}");
        }
    };

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
