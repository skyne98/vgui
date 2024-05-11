use std::{cell::RefCell, collections::HashSet, rc::Rc};

use color_eyre::owo_colors::OwoColorize;
use eframe::egui;
use eyre::{Context, ContextCompat, Result};
use mini_v8::{MiniV8, Value};

use colored::*;

fn value_to_string(
    isolate: &MiniV8,
    value: Value,
    depth: usize,
    seen: &mut HashSet<usize>,
    member: bool,
) -> Result<String> {
    let indent = "  ".repeat(depth);
    let next_indent = "  ".repeat(depth + 1);

    if value.is_null() {
        return Ok(format!("{}", "null".bold()));
    }

    if value.is_boolean() {
        let bool_value: bool = value.into(isolate).expect("Failed to convert boolean");
        return Ok(format!("{}", bool_value.to_string().yellow()));
    }

    if value.is_number() {
        let number_value: f64 = value.into(isolate).expect("Failed to convert number");
        return Ok(format!("{}", number_value.to_string().yellow()));
    }

    if value.is_string() {
        if member {
            let string_value: String = value.into(isolate).expect("Failed to convert string");
            return Ok(format!("\"{}\"", string_value).green().to_string());
        } else {
            let string_value: String = value.into(isolate).expect("Failed to convert string");
            return Ok(format!("{}", string_value));
        }
    }

    if value.is_function() {
        let function = value.as_function().wrap_err("Failed to get function")?;
        let function_name = function.name();
        let function_name = if function_name.is_empty() {
            "<anonymous>".to_string()
        } else {
            function_name
        };
        return Ok(format!("[Function: {}]", function_name)
            .bold()
            .cyan()
            .to_string());
    }

    if value.is_array() {
        let value_hash = value.hash(isolate);
        if seen.contains(&value_hash) {
            return Ok("[Circular]".bold().red().to_string());
        }
        seen.insert(value_hash);

        let array = value.as_array().wrap_err("Failed to get array")?;
        let length = array.len();

        if length == 0 {
            return Ok(format!("{}", "[]"));
        }

        let mut items = Vec::new();
        for i in 0..length {
            let item: Value = array.get(i).expect("Failed to get array item");
            let item_string = value_to_string(isolate, item, depth + 1, seen, true)?;
            items.push(item_string);
        }

        if length <= 3 && items.iter().map(|s| s.len()).sum::<usize>() <= 60 {
            let inline_items = items.join(", ");
            Ok(format!("{}", format!("[ {} ]", inline_items)))
        } else {
            let mut result = String::new();
            result.push_str(&format!("{}", "[\n"));

            for (i, item) in items.iter().enumerate() {
                result.push_str(&format!("{}{}", next_indent, item));

                if i < length as usize - 1 {
                    result.push_str(",\n");
                } else {
                    result.push('\n');
                }
            }

            result.push_str(&format!("{}{}", indent, "]"));
            Ok(result)
        }
    } else if value.is_object() {
        let value_hash = value.hash(isolate);
        if seen.contains(&value_hash) {
            return Ok("[Circular]".bold().red().to_string());
        }
        seen.insert(value_hash);

        let object = value.as_object().wrap_err("Failed to get object")?;
        let keys = object.keys(true).expect("Failed to get keys");
        let length = keys.len();

        if length == 0 {
            return Ok(format!("{}", "{}"));
        }

        let mut entries = Vec::new();
        for i in 0..length {
            let key: Value = keys.get(i).expect("Failed to get key");
            let value: Value = object.get(key.clone()).expect("Failed to get value");

            let key_string: String = key.into(isolate).expect("Failed to convert key");
            let value_string = value_to_string(isolate, value, depth + 1, seen, true)?;
            entries.push(format!("{}: {}", key_string.blue(), value_string));
        }

        if length <= 3 && entries.iter().map(|s| s.len()).sum::<usize>() <= 60 {
            let inline_entries = entries.join(", ");
            Ok(format!("{}", format!("{{ {} }}", inline_entries)))
        } else {
            let mut result = String::new();
            result.push_str(&format!("{}", "{\n"));

            for (i, entry) in entries.iter().enumerate() {
                result.push_str(&format!("{}{}", next_indent, entry));

                if i < length as usize - 1 {
                    result.push_str(",\n");
                } else {
                    result.push('\n');
                }
            }

            result.push_str(&format!("{}{}", indent, "}"));
            Ok(result)
        }
    } else {
        let value_string: String = value.into(isolate).expect("Failed to convert value");
        Ok(format!("{}{}", indent, value_string))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "vgui demo",
        native_options,
        Box::new(|cc| {
            Box::new(
                GuiApp::new(cc)
                    .wrap_err("Failed to create app")
                    .expect("Failed to create app"),
            )
        }),
    )
    .map_err(|e| eyre::eyre!(format!("{:?}", e)))
    .wrap_err("Failed to run eframe")?;

    Ok(())
}

struct GuiApp {
    isolate: MiniV8,
    value: Rc<RefCell<i32>>,
}

impl GuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        // initialize JS
        let isolate = MiniV8::new();
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        // hook up the console functions (log, warn, error, info)
        let rust_log_isolate = isolate.clone();
        let rust_log = isolate.create_function(move |invocation| {
            let args = invocation.args;
            let args: Vec<String> = args
                .iter()
                .map(|arg| {
                    value_to_string(
                        &rust_log_isolate,
                        arg.clone(),
                        0,
                        &mut HashSet::new(),
                        false,
                    )
                    .expect("Failed to convert")
                })
                .collect();
            let args = args.join(" ");
            println!("{}", args);
            Ok(())
        });
        let console_obj = isolate.create_object();
        console_obj
            .set("log", rust_log.clone())
            .expect("Failed to set log");
        console_obj
            .set("warn", rust_log.clone())
            .expect("Failed to set warn");
        console_obj
            .set("error", rust_log.clone())
            .expect("Failed to set error");
        console_obj
            .set("info", rust_log.clone())
            .expect("Failed to set info");
        isolate
            .global()
            .set("console", console_obj)
            .expect("Failed to set console");

        // Set value function
        let rust_set_value_isolate = isolate.clone();
        let value = Rc::new(RefCell::new(0));
        let value_clone = value.clone();
        let rust_set_value = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 1 {
                return Err(mini_v8::Error::ExternalError("Expected 1 argument".into()));
            }
            let value = args.get(0);
            let value: i32 = value
                .into(&rust_set_value_isolate)
                .expect("Failed to convert value");
            *value_clone.borrow_mut() = value;
            Ok(())
        });
        isolate
            .global()
            .set("setValue", rust_set_value)
            .expect("Failed to set setValue");

        // Set up the JS virtual machine
        let vue_code = include_str!("../assets/vue.global.js");
        isolate
            .eval::<_, Value>(vue_code)
            .expect("Failed to eval vue code");

        // Try to initialize the Vue app
        let vue_init_code = r#"
const { createRenderer, defineComponent, h, reactive } = Vue;

console.log('Vue version:', Vue.version);
console.log('Testing console.log');
console.log('Small array:', [1, 2, 3]);
console.log('Small object:', { a: 1 });

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
console.log(root);

// Send value to Rust
setValue(42);
"#;

        let result = isolate
            .eval::<_, Value>(vue_init_code)
            .expect("Failed to eval vue init code");

        // run the promise microtasks
        for _ in 0..10 {
            println!("=====================");
            println!("Running microtasks...");
            println!("=====================");
            let start = std::time::Instant::now();
            isolate.run_microtasks();
            let elapsed = start.elapsed();
            println!("=====================");
            println!("Microtasks took: {:?}", elapsed);
        }

        Ok(Self { isolate, value })
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World from eframe!");
            ui.label(format!("Value: {}", *self.value.borrow()));
        });
    }
}
