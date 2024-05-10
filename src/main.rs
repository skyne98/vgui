use eframe::egui;
use eyre::{Context, ContextCompat, Result};
use rusty_v8 as v8;

use colored::*;

fn value_to_string<'s>(
    scope: &mut v8::HandleScope<'s>,
    value: v8::Local<v8::Value>,
    depth: usize,
) -> Result<String> {
    const INDENT: &str = "  "; // Two spaces for each indentation level
    let indentation = INDENT.repeat(depth);
    let value_indentation = INDENT.repeat(depth + 1); // Increase indent for values

    if value.is_function() {
        let function = v8::Local::<v8::Function>::try_from(value).unwrap();
        let function_source = function
            .to_string(scope)
            .wrap_err("Failed to convert function to string")?
            .to_rust_string_lossy(scope);

        Ok(format!(
            "{} {}",
            "ƒ".bright_magenta(),
            function_source.bright_cyan(),
        ))
    } else if value.is_array() {
        let array = v8::Local::<v8::Array>::try_from(value).unwrap();
        let length = array.length();
        let mut result = String::new();

        result.push_str(&format!("{}[\n", indentation.cyan()));

        for i in 0..length {
            let js_index = v8::Integer::new(scope, i as i32);
            let element_value = array.get(scope, js_index.into()).unwrap();
            let element_str = value_to_string(scope, element_value, depth + 1)?;

            result.push_str(&format!("{}{},\n", value_indentation.cyan(), element_str));
        }

        result.push_str(&format!("{}]", indentation.cyan())); // Closing bracket with base indentation

        Ok(result)
    } else if value.is_object() {
        let object = value.to_object(scope).unwrap();
        let property_names = object.get_property_names(scope).unwrap();
        let mut result = String::new();

        result.push_str(&format!("{{\n{}", indentation.cyan()));

        for i in 0..property_names.length() {
            let js_index = v8::Integer::new(scope, i as i32);
            let key = property_names.get(scope, js_index.into()).unwrap();
            let key_str = key.to_string(scope).unwrap().to_rust_string_lossy(scope);
            let property_value = object.get(scope, key.into()).unwrap();
            let property_str = value_to_string(scope, property_value, depth + 1)?;

            result.push_str(&format!(
                "{}{}: {},\n",
                value_indentation.cyan(),
                key_str.green(),
                property_str
            ));
        }

        result.push_str(&format!("{}}}", indentation.cyan())); // Closing brace with base indentation

        Ok(result)
    } else {
        let maybe_string = value.to_string(scope);
        let string = maybe_string
            .context("Failed to convert value to string")?
            .to_string(scope)
            .wrap_err("Failed to convert value to string")?;
        Ok(string.to_rust_string_lossy(scope))
    }
}

fn log_function(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    let args_len = args.length();
    let mut result = String::new();
    for i in 0..args_len {
        let arg = args.get(i);
        let arg = value_to_string(scope, arg, 0).unwrap();
        result.push_str(&arg);
        if i < args_len - 1 {
            result.push_str(", ");
        }
    }
    println!("{}", result);
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
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        {
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
            let vue_code =
                v8::String::new(scope, vue_code).wrap_err("Failed to create JS string")?;
            let script =
                v8::Script::compile(scope, vue_code, None).wrap_err("Failed to compile script")?;
            let result = script.run(scope).wrap_err("Failed to run script")?;
            let result = result
                .to_string(scope)
                .wrap_err("Failed to convert result to string")?;
            println!("result (vue init): {}", result.to_rust_string_lossy(scope));

            // Try to initialize the Vue app
            let vue_init_code = r#"
    const { createRenderer, defineComponent, h, reactive } = Vue;

    const globalState = reactive({
        a: 1
    });

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
            let nextValueType = typeof nextValue;
            console.log(`Patching prop: ${key} from ${prevValue} to ${nextValue} of type ${nextValueType}`);
            el.attributes[key] = nextValue;
        },
        parentNode(node) {
            console.log(`Getting parent node of: ${node.tag}`);
            // Return the parent node
            return node;
        },
        nextSibling(node) {
            console.log(`Getting next sibling of: ${node.tag}`);
            // Return the next sibling node
            return node;
        },
        querySelector(selector) {
            console.log(`Querying selector: ${selector}`);
            // Return the first element that matches the selector
            return { tag: 'div', children: [], attributes: {} };
        },
    };

    const { render, createApp } = createRenderer(nodeOps);
    const { ref } = Vue;

    const App = {
        setup() {
            console.log('App setup:');
            const { ref, computed, nextTick } = Vue;

            const b = ref(2);
            const sum = computed(() => {
                console.log(`Computing sum: ${globalState.a} + ${b.value}`);
                return globalState.a + b.value;
            });

            // mount and unmount lifecycle hooks
            Vue.onMounted(() => {
                console.log('App mounted:');
                console.log(`a: ${globalState.a}`);
                console.log(`b: ${b.value}`);
            });
            Vue.onUnmounted(() => {
                console.log('App unmounted:');
                console.log(`a: ${globalState.a}`);
                console.log(`b: ${b.value}`);
            });

            function changeB() {
                b.value = 3;
            }

            nextTick(() => {
                console.log('Next tick:');
                console.log(`a: ${globalState.a}`);
                console.log(`b: ${b.value}`);
            });

            return { b, sum, changeB };
        },
        template: '<div id="main" style="color: red"><span @click="() => 2" @fire="() => 2">Hello, sum is: {{ sum }}</span></div>',
    };

    // The 'root' object would represent the top level of your app
    const root = { tag: 'app', children: [], attributes: {} };
    console.log(`Root object created: ${JSON.stringify(root, null, 2)}`);
    const appInstance = createApp(App).mount(root);

    // Update a and b values
    console.log(appInstance);
    globalState.a = 2;
    appInstance.changeB();

    // For demonstration, log the root object to see the structure
    function circularReplacer() {
        const seen = new WeakSet();
        return (key, value) => {
            if (key === '_vnode') {
                return '[VNode]'; // Indicate virtual node
            }
            if (key === '__vue_app__') {
                return '[VueApp]'; // Indicate Vue app
            }
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
            let script = v8::Script::compile(scope, vue_init_code, None)
                .wrap_err("Failed to compile script")?;
            let result = script.run(scope).wrap_err("Failed to run script")?;
            let result = result
                .to_string(scope)
                .wrap_err("Failed to convert result to string")?;
            println!("result (app init): {}", result.to_rust_string_lossy(scope));

            // check for background tasks
            let has_tasks = scope.has_pending_background_tasks();
            println!("Has pending background tasks: {}", has_tasks);
            // run the promise microtasks
            for _ in 0..10 {
                println!("=====================");
                println!("Running microtasks...");
                println!("=====================");
                let start = std::time::Instant::now();
                scope.perform_microtask_checkpoint();
                let elapsed = start.elapsed();
                println!("=====================");
                println!("Microtasks took: {:?}", elapsed);
            }
        }
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
