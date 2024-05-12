use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use color_eyre::owo_colors::OwoColorize;
use eframe::egui;
use eyre::{Context, ContextCompat, Result};
use mini_v8::{Error as MiniV8Error, MiniV8, ToValue, Value};

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

#[derive(Debug, Clone)]
enum Element {
    Root,
    Label(String),
    Vertical,
    Horizontal,
}
type ElementRef = Rc<RefCell<Element>>;
type ElementId = usize;
type Elements = HashMap<ElementId, ElementRef>;
type ElementsRef = Rc<RefCell<Elements>>;
type ElementsVec = Vec<ElementId>;
type ElementsChildren = HashMap<ElementId, ElementsVec>;
type ElementsChildrenRef = Rc<RefCell<ElementsChildren>>;

struct GuiApp {
    isolate: MiniV8,
    value: Rc<RefCell<i32>>,
    elements: ElementsRef,
    elements_children: ElementsChildrenRef,
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
                return Err(MiniV8Error::ExternalError("Expected 1 argument".into()));
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

        // Virtual DOM CRUD
        let elements = Rc::new(RefCell::new(HashMap::new()));
        elements
            .borrow_mut()
            .insert(0, Rc::new(RefCell::new(Element::Root)));
        let elements_children = Rc::new(RefCell::new(HashMap::new()));

        // Create element (createElement)
        let rust_node_ops_isolate = isolate.clone();
        let elements_clone = elements.clone();
        let rust_create_element = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 2 {
                return Err(MiniV8Error::ExternalError("Expected 2 argument".into()));
            }

            let id = args.get(0);
            let id: ElementId = id
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert id");
            let tag = args.get(1);
            let tag: String = tag
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert tag");

            println!("---------------------");
            println!("Creating element: {}", tag);

            match tag.as_str() {
                "label" => {
                    let element = Element::Label("".to_string());
                    let element_ref = Rc::new(RefCell::new(element));
                    let element_id = id as ElementId;
                    elements_clone.borrow_mut().insert(element_id, element_ref);
                }
                "vertical" => {
                    let element = Element::Vertical;
                    let element_ref = Rc::new(RefCell::new(element));
                    let element_id = id as ElementId;
                    elements_clone.borrow_mut().insert(element_id, element_ref);
                }
                "horizontal" => {
                    let element = Element::Horizontal;
                    let element_ref = Rc::new(RefCell::new(element));
                    let element_id = id as ElementId;
                    elements_clone.borrow_mut().insert(element_id, element_ref);
                }
                _ => {
                    return Err(MiniV8Error::ExternalError(
                        format!("Unknown tag: {}", tag).into(),
                    ));
                }
            }

            let element = (*elements_clone
                .borrow()
                .get(&id)
                .expect("Failed to get element")
                .borrow())
            .clone();
            println!("Element created: {:?}", element);
            println!("---------------------");

            Ok(id)
        });
        isolate
            .global()
            .set("createElement", rust_create_element)
            .expect("Failed to set createElement");
        // Insert element (insertElement)
        let rust_node_ops_isolate = isolate.clone();
        let elements_clone = elements.clone();
        let elements_children_clone = elements_children.clone();
        let rust_insert = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 3 {
                return Err(MiniV8Error::ExternalError("Expected 3 arguments".into()));
            }
            let child = args.get(0);
            let parent = args.get(1);
            let anchor = args.get(2);
            let child: ElementId = child
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert child");
            let parent: ElementId = parent
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert parent");
            let anchor: Option<ElementId> = anchor
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert anchor");

            let elements_borrow = elements_clone.borrow();
            let child_element = elements_borrow.get(&child).expect("Failed to get child");
            let parent_element = elements_borrow.get(&parent).expect("Failed to get parent");
            let anchor_element = anchor.map(|id| elements_borrow.get(&id)).flatten();

            println!("---------------------");
            println!("Inserting element: {:?}", child_element);
            println!("++ Parent: {:?}", parent_element);
            println!("++ Anchor: {:?}", anchor_element);
            println!("---------------------");

            let mut elements_children_borrow = elements_children_clone.borrow_mut();
            let parent_children = elements_children_borrow
                .entry(parent)
                .or_insert_with(Vec::new);
            let anchor_index = anchor
                .map(|anchor| {
                    parent_children
                        .iter()
                        .position(|id| id == &anchor)
                        .expect("Failed to get anchor index")
                })
                .unwrap_or(parent_children.len());
            parent_children.insert(anchor_index, child);

            Ok(())
        });
        isolate
            .global()
            .set("insertElement", rust_insert)
            .expect("Failed to set insert");

        // Set element text (setElementText)
        let rust_node_ops_isolate = isolate.clone();
        let elements_clone = elements.clone();
        let rust_set_element_text = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 2 {
                return Err(MiniV8Error::ExternalError("Expected 2 arguments".into()));
            }
            let element = args.get(0);
            let text = args.get(1);
            let element: ElementId = element
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert element");
            let text: String = text
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert text");

            let elements_borrow = elements_clone.borrow();
            let element_ref = elements_borrow
                .get(&element)
                .expect("Failed to get element");

            let mut element_mut = element_ref.borrow_mut();
            println!("---------------------");
            println!("Setting element text: {:?} to {}", element_mut, text);
            println!("---------------------");
            match &mut *element_mut {
                Element::Label(label) => {
                    *label = text.clone();
                }
                _ => {
                    return Err(MiniV8Error::ExternalError(
                        format!("Cannot set text on element: {:?}", element_mut).into(),
                    ));
                }
            }

            Ok(element)
        });
        isolate
            .global()
            .set("setElementText", rust_set_element_text)
            .expect("Failed to set setElementText");

        // Get parent node (parentNode)
        let rust_node_ops_isolate = isolate.clone();
        let element_children_clone = elements_children.clone();
        let rust_parent_node = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 1 {
                return Err(MiniV8Error::ExternalError("Expected 1 argument".into()));
            }
            let node = args.get(0);
            let node: ElementId = node
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert node");

            println!("---------------------");
            println!("Getting parent node of: {}", node);
            println!("---------------------");

            let children_borrow = element_children_clone.borrow();
            for (parent, children) in children_borrow.iter() {
                if children.contains(&node) {
                    return Ok((*parent)
                        .to_value(&rust_node_ops_isolate)
                        .expect("Failed to convert"));
                }
            }

            Ok(Value::Null)
        });
        isolate
            .global()
            .set("parentNode", rust_parent_node)
            .expect("Failed to set parentNode");

        // Get next sibling (nextSibling)
        let rust_node_ops_isolate = isolate.clone();
        let element_children_clone = elements_children.clone();
        let rust_next_sibling = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 1 {
                return Err(MiniV8Error::ExternalError("Expected 1 argument".into()));
            }
            let node = args.get(0);
            let node: ElementId = node
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert node");

            println!("---------------------");
            println!("Getting next sibling of: {}", node);
            println!("---------------------");

            let children_borrow = element_children_clone.borrow();
            for (_, children) in children_borrow.iter() {
                if let Some(index) = children.iter().position(|id| id == &node) {
                    if index < children.len() - 1 {
                        return Ok((children[index + 1])
                            .to_value(&rust_node_ops_isolate)
                            .expect("Failed to convert"));
                    }
                }
            }

            Ok(Value::Null)
        });
        isolate
            .global()
            .set("nextSibling", rust_next_sibling)
            .expect("Failed to set nextSibling");

        // Set up the JS virtual machine
        let vue_code = include_str!("../assets/vue.global.js");
        isolate
            .eval::<_, Value>(vue_code)
            .expect("Failed to eval vue code");

        // Try to initialize the Vue app
        let vue_init_code = r#"
try {
    const { createRenderer, defineComponent, h, reactive } = Vue;

    console.log('Vue version:', Vue.version);
    console.log('Testing console.log');
    console.log('Small array:', [1, 2, 3]);
    console.log('Small object:', { a: 1 });

    const globalState = reactive({
        a: 1
    });

    const elementToId = new Map();
    let nextId = 1;
    function getId(element) {
        if (!elementToId.has(element)) {
            elementToId.set(element, nextId++);
        }
        return elementToId.get(element);
    }

    const nodeOps = {
        // Create a node in the non-DOM environment
        createElement(tag) {
            const id = nextId++; 
            let element = { id: createElement(id, tag) };
            elementToId.set(element, id);
            return element;
        },
        // Insert child into parent, possibly using some custom API
        insert(child, parent, anchor) {
            insertElement(child.id, parent.id, anchor);
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
            setElementText(el.id, text);
        },
        patchProp(el, key, prevValue, nextValue) {
            let nextValueType = typeof nextValue;
            console.log(`Patching prop: ${key} from ${prevValue} to ${nextValue} of type ${nextValueType}`);
            el.attributes[key] = nextValue;
        },
        parentNode(node) {
            return getId(parentNode(node.id));
        },
        nextSibling(node) {
            return getId(nextSibling(node.id));
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
            const a = computed(() => {
                return globalState.a;
            });

            // mount and unmount lifecycle hooks
            Vue.onMounted(() => {
                console.log('App mounted:', { a: globalState.a, b: b.value });
            });
            Vue.onUnmounted(() => {
                console.log('App unmounted:', { a: globalState.a, b: b.value });
            });

            function changeB() {
                b.value = 3;
            }

            nextTick(() => {
                console.log('Next tick:', { a: globalState.a, b: b.value });
            });

            return { a, b, sum, changeB };
        },
        template: `
            <vertical>
                <horizontal>
                    <label>Value of a:</label>
                    <label>{{ a }}</label>
                </horizontal>
                <label>Value of b: {{ b }}</label>
                <label>Sum: {{ sum }}</label>
            </vertical>
        `,
    };

    // The 'root' object would represent the top level of your app
    const root = { id: 0 };
    console.log(`Root object created: ${JSON.stringify(root, null, 2)}`);
    const appInstance = createApp(App).mount(root);

    // Update a and b values
    console.log(appInstance);
    globalState.a = 2;
    appInstance.changeB();

    // Send value to Rust
    setValue(42);
} catch (e) {
    const errorMessage = `Error Message: ${e.message}`;
    const stackTrace = `Stack Trace:\n${e.stack}`;

    console.error(`${errorMessage}\n${stackTrace}`);
}
"#;

        isolate
            .eval::<_, Value>(vue_init_code)
            .map_err(|e| eyre::eyre!(format!("MiniV8 error: {:#?}", e)))?;

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

        let this = Self {
            isolate,
            value,
            elements,
            elements_children,
        };
        this.print_tree(0, 0);
        Ok(this)
    }

    // Element related functions
    fn print_tree(&self, element_id: ElementId, depth: usize) {
        let indent = "  ".repeat(depth);
        let elements_borrow = self.elements.borrow();
        let element_ref = elements_borrow
            .get(&element_id)
            .expect("Failed to get element");
        let element = element_ref.borrow();
        match &*element {
            Element::Root => {
                println!("{}Root", indent);
            }
            Element::Label(label) => {
                println!("{}Label: {}", indent, label);
            }
            Element::Vertical => {
                println!("{}Vertical", indent);
            }
            Element::Horizontal => {
                println!("{}Horizontal", indent);
            }
        }

        let elements_children_borrow = self.elements_children.borrow();
        let children = elements_children_borrow.get(&element_id);
        if let Some(children) = children {
            for child_id in children {
                self.print_tree(*child_id, depth + 1);
            }
        }

        println!("{}End", indent);
    }

    // Walking the tree with a stack of contexts
    // Will be used later for rendering with eframe/egui
    fn render_element(&self, ui: &mut egui::Ui, element_id: ElementId) {
        let elements_borrow = self.elements.borrow();
        let element_ref = elements_borrow
            .get(&element_id)
            .expect("Failed to get element");
        let element = element_ref.borrow();
        match &*element {
            Element::Root => {
                let elements_children_borrow = self.elements_children.borrow();
                let children = elements_children_borrow.get(&element_id);
                if let Some(children) = children {
                    for child_id in children {
                        self.render_element(ui, *child_id);
                    }
                }
            }
            Element::Label(label) => {
                ui.label(label);
            }
            Element::Vertical => {
                ui.vertical(|ui| {
                    let elements_children_borrow = self.elements_children.borrow();
                    let children = elements_children_borrow.get(&element_id);
                    if let Some(children) = children {
                        for child_id in children {
                            self.render_element(ui, *child_id);
                        }
                    }
                });
            }
            Element::Horizontal => {
                ui.horizontal(|ui| {
                    let elements_children_borrow = self.elements_children.borrow();
                    let children = elements_children_borrow.get(&element_id);
                    if let Some(children) = children {
                        for child_id in children {
                            self.render_element(ui, *child_id);
                        }
                    }
                });
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_element(ui, 0);
        });
    }
}
