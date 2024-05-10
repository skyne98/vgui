use eframe::egui;
use eyre::{Context, ContextCompat, Result};
use rusty_v8 as v8;

fn log_function(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    for i in 0..args.length() {
        let arg = args.get(i);
        let arg = arg.to_string(scope).unwrap();
        let arg = arg.to_rust_string_lossy(scope);
        println!("{}", arg);
    }
}
// hook up a named function to the console object
fn hook_function(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
    name: &str,
    function: impl v8::MapFnTo<v8::FunctionCallback>,
) -> Result<()> {
    let name = v8::String::new(scope, name.into()).wrap_err("Failed to create string")?;
    let function = v8::FunctionTemplate::new(scope, function);
    let function = function
        .get_function(scope)
        .wrap_err("Failed to create function")?;
    global
        .set(scope, name.into(), function.into())
        .wrap_err("Failed to set function")?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // Instantiate the execution context
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    {
        let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
        let handle_scope = &mut v8::HandleScope::new(isolate);
        let context = v8::Context::new(handle_scope);
        let scope = &mut v8::ContextScope::new(handle_scope, context);

        let global = context.global(scope);
        let console_name =
            v8::String::new(scope, "console".into()).wrap_err("Failed to create string")?;
        let console = v8::Object::new(scope);
        global
            .set(scope, console_name.into(), console.into())
            .wrap_err("Failed to set console object")?;
        // Set up the console functions
        hook_function(scope, console, "log", log_function)?;
        hook_function(scope, console, "info", log_function)?;
        hook_function(scope, console, "warn", log_function)?;
        hook_function(scope, console, "error", log_function)?;

        // Set up the JS virtual machine
        let vue_code = include_str!("../assets/vue.global.js");
        let vue_code = v8::String::new(scope, vue_code).wrap_err("Failed to create JS string")?;
        let script =
            v8::Script::compile(scope, vue_code, None).wrap_err("Failed to compile script")?;
        let result = script.run(scope).wrap_err("Failed to run script")?;
        let result = result
            .to_string(scope)
            .wrap_err("Failed to convert result to string")?;
        println!("result (vue init): {}", result.to_rust_string_lossy(scope));

        // Try to initialize the Vue app
        let vue_init_code = r#"
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
        setElementText(el, text) {
            console.log(`Setting element text: ${text}`);
            el.children = [{ type: 'text', text }];
        },
        patchProp(el, key, prevValue, nextValue) {
            console.log(`Patching prop: ${key} from ${prevValue} to ${nextValue}`);
            el.attributes[key] = nextValue;
        }
    };

    const { render, createApp } = createRenderer(nodeOps);

    const App = {
        setup() {
            const { ref, computed } = Vue;

            const a = ref(1);
            const b = ref(2);
            const sum = computed(() => a.value + b.value);

            return { sum };
        },
        template: '<div id="main" style="color: red"><span>Hello, sum is: {{ sum }}</span></div>',
    };

    // The 'root' object would represent the top level of your app
    const root = { tag: 'app', children: [], attributes: {} };
    console.log(`Root object created: ${JSON.stringify(root, null, 2)}`);
    createApp(App).mount(root);
    console.log('App mounted');

    // For demonstration, log the root object to see the structure
    function circularReplacer() {
        const seen = new WeakSet();
        return (key, value) => {
            if (typeof value === 'object' && value !== null) {
                if (seen.has(value)) {
                    return '[Circular]'; // Indicate circular reference
                }
                seen.add(value);
            }
            return value;
        };
    }
    console.log(JSON.stringify(root, circularReplacer(), 2));
"#;

        let vue_init_code =
            v8::String::new(scope, vue_init_code).wrap_err("Failed to create JS string")?;
        let script =
            v8::Script::compile(scope, vue_init_code, None).wrap_err("Failed to compile script")?;
        let result = script.run(scope).wrap_err("Failed to run script")?;
        let result = result
            .to_string(scope)
            .wrap_err("Failed to convert result to string")?;
        println!("result (app init): {}", result.to_rust_string_lossy(scope));
    }

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
