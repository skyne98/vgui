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
    match context.eval(Source::from_bytes(
        r#"
        const { createRenderer, defineComponent, h } = Vue;

        const nodeOps = {
            // Create a node in the non-DOM environment
            createElement(tag) {
                console.log(`Creating element: ${tag}`);
                // Return an object that represents your element
                return { tag, children: [], attributes: {} };
            },
            // Insert child into parent, possibly using some custom API
            insert(child, parent, anchor) {
                console.log(`Inserting element: ${child.tag}`);
                parent.children.push(child);
            },
            // Remove an element, adapting to your backend's capabilities
            remove(child) {
                console.log(`Removing element: ${child.tag}`);
                // Implement removal logic according to your environment
            },
            createText(text) {
                console.log(`Creating text node: ${text}`);
                return { type: 'text', text };
            },
            setText(node, text) {
                console.log(`Setting text: ${text}`);
                node.text = text;
            },
            patchProp(el, key, prevValue, nextValue) {
                console.log(`Patching prop: ${key} from ${prevValue} to ${nextValue}`);
                el.attributes[key] = nextValue;
            }
        };

        const { render, createApp } = createRenderer(nodeOps);
        console.log(`Renderer created: ${render}`);
        console.log(`App creator created: ${createApp}`);

        const App = {
            render() {
                return h('div', { id: 'main', style: 'color: red' }, [
                    h('span', null, 'Hello, custom environment!')
                ]);
            }
        };

        // The 'root' object would represent the top level of your app
        const root = { tag: 'app', children: [], attributes: {} };
        console.log(`Root object created: ${JSON.stringify(root, null, 2)}`);
        createApp(App).mount(root);

        // For demonstration, log the root object to see the structure
        console.log(JSON.stringify(root, null, 2));
    "#,
    )) {
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
