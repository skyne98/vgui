use eframe::egui;
use eyre::{Context, ContextCompat, Result};
use mini_v8::{MiniV8, ToValue, Value};

use colored::*;

fn value_to_string(isolate: &MiniV8, value: Value, depth: usize) -> Result<String> {
    let mut result = String::new();
    let indent = "  ".repeat(depth);
    if value.is_object() {
        result.push_str(&format!("{}Object\n", indent).green().bold());
        let object = value.as_object().wrap_err("Failed to get object")?;
        let keys = object.keys(true).expect("Failed to get keys");
        for key in 0..keys.len() {
            let key: Value = keys.get(key).expect("Failed to get key");
            let value: Value = object.get(key.clone()).expect("Failed to get value");

            let key_string: String = key.into(isolate).expect("Failed to convert key");
            result.push_str(&format!("{}{}: ", indent, key_string));
            let value_string = value_to_string(isolate, value, depth + 1)?;
            result.push_str(&value_string);
        }
    } else {
        let value_string: String = value.into(isolate).expect("Failed to convert value");
        result.push_str(&format!("{}{}\n", indent, value_string));
    }

    Ok(result)
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
}

impl GuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        // initialize JS
        let isolate = MiniV8::new();
        // hook up the console functions (log, warn, error, info)
        let rust_log_isolate = isolate.clone();
        let rust_log = isolate.create_function(move |invocation| {
            let args = invocation.args;
            let args: Vec<String> = args
                .iter()
                .map(|arg| {
                    value_to_string(&rust_log_isolate, arg.clone(), 0).expect("Failed to convert")
                })
                .collect();
            let args = args.join(", ");
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

        // Set up the JS virtual machine
        let vue_code = include_str!("../assets/vue.global.js");
        isolate
            .eval::<_, Value>(vue_code)
            .expect("Failed to eval vue code");

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

        Ok(Self { isolate })
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
        });
    }
}
