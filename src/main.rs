use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use color_eyre::owo_colors::OwoColorize;
use eframe::egui::{self, Response};
use eyre::{Context, ContextCompat, Result};
use mini_v8::{Error as MiniV8Error, Function, MiniV8, ToValue, Value, Variadic};

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
    Hidden(String),
    Comment(String),
    Label(String),
    Button(String),
    Vertical,
    Horizontal,
    Separator,
    TextEdit(String),
}
struct Events {
    click: Option<Function>,
    hover: Option<Function>,
    input: Option<Function>,
}

type ElementRef = Rc<RefCell<Element>>;
type ElementId = usize;
type Elements = HashMap<ElementId, ElementRef>;
type ElementsRef = Rc<RefCell<Elements>>;
type ElementsVec = Vec<ElementId>;
type ElementsChildren = HashMap<ElementId, ElementsVec>;
type ElementsChildrenRef = Rc<RefCell<ElementsChildren>>;
type ElementEvents = HashMap<ElementId, Events>;
type ElementEventsRef = Rc<RefCell<ElementEvents>>;

struct GuiApp {
    isolate: MiniV8,
    elements: ElementsRef,
    elements_children: ElementsChildrenRef,
    element_events: ElementEventsRef,
}

macro_rules! define_js_function {
    ($isolate:expr, $name:expr, $arg_len:expr, |$($arg_name:ident: $arg_type:ty),*| $body:expr) => {
        {
            let isolate_clone = $isolate.clone();
            let function = $isolate.create_function(move |invocation| {
                if invocation.args.len() != $arg_len {
                    return Err(MiniV8Error::ExternalError(format!("Expected {} arguments", $arg_len).into()));
                }

                let mut arg_idx = 0;
                $(
                    let $arg_name: $arg_type = invocation.args.get(arg_idx).into(&isolate_clone).expect(&format!("Failed to convert argument {}", arg_idx));
                    arg_idx += 1;
                )*

                $body
            });
            $isolate.global().set($name, function).expect(&format!("Failed to set {}", $name));
        }
    };
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

        // Virtual DOM CRUD
        let elements = Rc::new(RefCell::new(HashMap::new()));
        elements
            .borrow_mut()
            .insert(0, Rc::new(RefCell::new(Element::Root)));
        let elements_children: Rc<RefCell<HashMap<usize, Vec<usize>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let element_events = Rc::new(RefCell::new(HashMap::new()));

        // Create element (createElement)
        let elements_clone = elements.clone();
        define_js_function!(isolate, "createElement", 2, |id: ElementId, tag: String| {
            println!("Creating element: {}", tag);
            match tag.as_str() {
                "label" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Label("".to_string()))));
                }
                "vertical" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Vertical)));
                }
                "horizontal" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Horizontal)));
                }
                "button" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Button("".to_string()))));
                }
                "hidden" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Hidden("".to_string()))));
                }
                "comment" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Comment("".to_string()))));
                }
                "separator" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::Separator)));
                }
                "text-edit" => {
                    elements_clone
                        .borrow_mut()
                        .insert(id, Rc::new(RefCell::new(Element::TextEdit("".to_string()))));
                }
                _ => {
                    return Err(MiniV8Error::ExternalError(
                        format!("Unknown tag: {}", tag).into(),
                    ));
                }
            }
            Ok(id)
        });

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
            println!("Inserting element: {} - {:?}", child, child_element);
            println!("++ Parent: {} - {:?}", parent, parent_element);
            println!("++ Anchor: {:?} - {:?}", anchor, anchor_element);
            println!("---------------------");

            // Ensure the child is not already inserted elsewhere
            let mut elements_children_borrow = elements_children_clone.borrow_mut();
            for (parent_id, children) in elements_children_borrow.iter_mut() {
                if children.contains(&child) {
                    panic!(
                        "Child element {} is already a child of parent element {}",
                        child, parent_id
                    );
                }
            }

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

        // Remove element (removeElement)
        let rust_node_ops_isolate = isolate.clone();
        let elements_clone = elements.clone();
        let elements_children_clone = elements_children.clone();
        let rust_remove = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 1 {
                return Err(MiniV8Error::ExternalError("Expected 1 argument".into()));
            }
            let child = args.get(0);
            let child: ElementId = child
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert child");

            // find parent
            let mut parent = None;
            let mut elements_children_borrow = elements_children_clone.borrow_mut();
            for (parent_id, children) in elements_children_borrow.iter_mut() {
                if let Some(index) = children.iter().position(|id| id == &child) {
                    parent = Some(*parent_id);
                    children.remove(index);
                    break;
                }
            }

            let elements_borrow = elements_clone.borrow();
            let child_element = elements_borrow.get(&child).expect("Failed to get child");
            let parent_element = parent
                .map(|id| elements_borrow.get(&id))
                .flatten()
                .expect("Failed to get parent");

            println!("---------------------");
            println!("Removing element: {} - {:?}", child, child_element);
            println!("++ Parent: {:?} - {:?}", parent, parent_element);
            println!("---------------------");

            Ok(())
        });
        isolate
            .global()
            .set("removeElement", rust_remove)
            .expect("Failed to set removeElement");

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
            println!(
                "Setting element text: {} - {:?} to {}",
                element, element_mut, text
            );
            println!("---------------------");
            match &mut *element_mut {
                Element::Label(label) => {
                    *label = text.clone();
                }
                Element::Button(label) => {
                    *label = text.clone();
                }
                Element::Hidden(label) => {
                    *label = text.clone();
                }
                Element::Comment(comment) => {
                    *comment = text.clone();
                }
                Element::TextEdit(label) => {
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
        let elements_clone = elements.clone();
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
            let elements_borrow = elements_clone.borrow();
            let node_element = elements_borrow
                .get(&node)
                .expect("Failed to get node element");

            println!("---------------------");
            println!("Getting parent node of: {} - {:?}", node, node_element);
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
        let elements_clone = elements.clone();
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
            let elements_borrow = elements_clone.borrow();
            let node_element = elements_borrow
                .get(&node)
                .expect("Failed to get node element");

            println!("---------------------");
            println!("Getting next sibling of: {} - {:?}", node, node_element);

            let children_borrow = element_children_clone.borrow();
            let mut sibling = Value::Null;
            for (_, children) in children_borrow.iter() {
                if let Some(index) = children.iter().position(|id| id == &node) {
                    if index < children.len() - 1 {
                        sibling = (children[index + 1])
                            .to_value(&rust_node_ops_isolate)
                            .expect("Failed to convert");
                    }
                }
            }

            println!("Next sibling: {:?}", sibling);
            println!("---------------------");

            Ok(sibling)
        });
        isolate
            .global()
            .set("nextSibling", rust_next_sibling)
            .expect("Failed to set nextSibling");

        // Property patching (patchProp)
        let rust_node_ops_isolate = isolate.clone();
        let elements_clone = elements.clone();
        let elements_events_clone = element_events.clone();
        let rust_patch_prop = isolate.create_function(move |invocation| {
            let args = invocation.args;
            if args.len() != 4 {
                return Err(MiniV8Error::ExternalError("Expected 4 arguments".into()));
            }
            let element = args.get(0);
            let key = args.get(1);
            let prev_value = args.get(2);
            let next_value = args.get(3);
            let element: ElementId = element
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert element");
            let key: String = key
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert key");
            let prev_value: Value = prev_value
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert prev_value");
            let next_value: Value = next_value
                .into(&rust_node_ops_isolate)
                .expect("Failed to convert next_value");

            let elements_borrow = elements_clone.borrow();
            let element_ref = elements_borrow
                .get(&element)
                .expect("Failed to get element");

            let element_mut = element_ref.borrow_mut();
            println!("---------------------");
            println!(
                "Patching prop: {} from {:?} to {:?}",
                key, prev_value, next_value
            );
            println!("Element: {} - {:?}", element, element_mut);
            println!("---------------------");

            // Check for events (onClick, onHover)
            let mut events_borrow = elements_events_clone.borrow_mut();
            // create the events object if it doesn't exist
            let events = events_borrow.entry(element).or_insert_with(|| Events {
                click: None,
                hover: None,
                input: None,
            });
            // now add or remove the event
            match key.as_str() {
                "onClick" => {
                    if next_value.is_function() {
                        events.click = Some(next_value.as_function().unwrap().clone());
                    } else {
                        events.click = None;
                    }
                }
                "onHover" => {
                    if next_value.is_function() {
                        events.hover = Some(next_value.as_function().unwrap().clone());
                    } else {
                        events.hover = None;
                    }
                }
                "onInput" => {
                    if next_value.is_function() {
                        events.input = Some(next_value.as_function().unwrap().clone());
                    } else {
                        events.input = None;
                    }
                }
                _ => {}
            }

            Ok(())
        });
        isolate
            .global()
            .set("patchProp", rust_patch_prop)
            .expect("Failed to set patchProp");

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

    const elementToId = new Map();
    const idToElement = new Map();
    let nextId = 1;
    function getId(element) {
        if (!elementToId.has(element)) {
            elementToId.set(element, nextId++);
        }
        return elementToId.get(element);
    }
    function getElementById(id) {
        return idToElement.get(id);
    }

    const nodeOps = {
        // Create a node in the non-DOM environment
        createElement(tag) {
            const id = nextId++; 
            let element = { id: createElement(id, tag) };
            elementToId.set(element, id);
            idToElement.set(id, element);
            return element;
        },
        // Insert child into parent, possibly using some custom API
        insert(child, parent, anchor) {
            insertElement(child.id, parent.id, anchor?.id);
        },
        // Remove an element, adapting to your backend's capabilities
        remove(child) {
            removeElement(child.id);
        },
        createText(text) {
            const id = nextId++;
            let element = { id: createElement(id, 'hidden') };
            setElementText(element.id, text);
            return element;
        },
        createComment(text) {
            const id = nextId++;
            let element = { id: createElement(id, 'comment') };
            setElementText(element.id, text);
            return element;
        },
        setText(node, text) {
            console.log('Setting text for node:', node);
            setElementText(node.id, text);
        },
        setElementText(el, text) {
            setElementText(el.id, text);
        },
        patchProp(el, key, prevValue, nextValue) {
            patchProp(el.id, key, prevValue, nextValue);
        },
        parentNode(node) {
            return getElementById(parentNode(node.id));
        },
        nextSibling(node) {
            return getElementById(nextSibling(node.id));
        },
        querySelector(selector) {
            throw new Error(`Not implemented, trying to query selector: ${selector}`);
        },
    };

    const { render, createApp } = createRenderer(nodeOps);
    const { watch, ref } = Vue;

    const App = {
        setup() {
            console.log('App setup:');

            const value = ref(0);
            const stringValue = ref('Hello, Vue!');
            const additionalControls = ref(false);

            const label = ref(null);
            watch(label, (value) => {
                console.log('Label changed:', value);
            });

            return { value, additionalControls, label, stringValue };
        },
        template: `
            <vertical>
                <label ref="label">Value: {{ value }}</label>
                <button @click="value++">Increment</button>
                <button @click="value = 0">Reset</button>
                <label>String Value: {{ stringValue }}</label>

                // Additional controls
                <button @click="additionalControls = !additionalControls">Toggle Controls</button>
                <vertical v-if="additionalControls">
                    <label>Additional Controls</label>
                    <button @click="value--">Decrement</button>
                    <text-edit @input="(v) => stringValue = v">{{ stringValue }}</text-edit>
                </vertical>
                <separator></separator>
            </vertical>
        `,
    };

    // The 'root' object would represent the top level of your app
    const root = { id: 0 };
    console.log(`Root object created:`, root);
    const unmountedApp = createApp(App);
    unmountedApp.config.isCustomElement = tag => {
        console.log(`Checking if ${tag} is a custom element`);
        return [
            'label',
            'vertical',
            'horizontal',
            'button',
            'hidden',
            'comment',
            'separator',
            'text-edit',
        ].includes(tag);
    };
    const appInstance = unmountedApp.mount(root);
} catch (e) {
    const errorMessage = `Error Message: ${e.message}`;
    const stackTrace = `Stack Trace:\n${e.stack}`;

    console.error(`${errorMessage}\n${stackTrace}`);
}
"#;

        isolate
            .eval::<_, Value>(vue_init_code)
            .map_err(|e| eyre::eyre!(format!("MiniV8 error: {:#?}", e)))?;

        let this = Self {
            isolate,
            elements,
            elements_children,
            element_events,
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
                println!("{}Root({})", indent, element_id);
            }
            Element::Label(label) => {
                println!("{}Label({}): {}", indent, element_id, label);
            }
            Element::Button(label) => {
                println!("{}Button({}): {}", indent, element_id, label);
            }
            Element::Vertical => {
                println!("{}Vertical({})", indent, element_id);
            }
            Element::Horizontal => {
                println!("{}Horizontal({})", indent, element_id);
            }
            Element::Hidden(label) => {
                println!("{}Hidden({}): {}", indent, element_id, label);
            }
            Element::Comment(comment) => {
                println!("{}Comment({}): {}", indent, element_id, comment);
            }
            Element::Separator => {
                println!("{}Separator({})", indent, element_id);
            }
            Element::TextEdit(label) => {
                println!("{}TextEdit({}): {}", indent, element_id, label);
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
    fn render_element(&self, ui: &mut egui::Ui, element_id: ElementId) -> Vec<Response> {
        let elements_borrow = self.elements.borrow();
        let element_ref = elements_borrow
            .get(&element_id)
            .expect("Failed to get element");
        let mut element = element_ref.borrow_mut();
        let mut responses = Vec::new();

        match &mut *element {
            Element::Root => {
                let elements_children_borrow = self.elements_children.borrow();
                let children = elements_children_borrow.get(&element_id);
                if let Some(children) = children {
                    for child_id in children {
                        let local_responses = self.render_element(ui, *child_id);
                        responses.extend(local_responses);
                    }
                }
            }
            Element::Label(label) => responses.push(ui.label(label.clone())),
            Element::Button(label) => responses.push(ui.button(label.clone())),
            Element::Hidden(_) => { /* do nothing */ }
            Element::Comment(_) => { /* do nothing */ }
            Element::Vertical => {
                ui.vertical(|ui| {
                    let elements_children_borrow = self.elements_children.borrow();
                    let children = elements_children_borrow.get(&element_id);
                    if let Some(children) = children {
                        for child_id in children {
                            let local_responses = self.render_element(ui, *child_id);
                            responses.extend(local_responses);
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
                            let local_responses = self.render_element(ui, *child_id);
                            responses.extend(local_responses);
                        }
                    }
                });
            }
            Element::Separator => {
                ui.separator();
            }
            Element::TextEdit(label) => {
                let response = ui.text_edit_singleline(label);
                responses.push(response);
            }
        }

        // Hook up events
        let element_events_borrow = self.element_events.borrow();
        let events = element_events_borrow.get(&element_id);
        if let Some(events) = events {
            for response in &responses {
                if let Some(click) = &events.click {
                    if response.clicked() {
                        click
                            .call::<(), ()>(().into())
                            .expect("Failed to call click event");
                    }
                }
                if let Some(hover) = &events.hover {
                    if response.hovered() {
                        hover
                            .call::<(), ()>(().into())
                            .expect("Failed to call hover event");
                    }
                }
                if let Some(input) = &events.input {
                    if let Element::TextEdit(label) = &*element {
                        if response.lost_focus() {
                            input
                                .call::<Variadic<Value>, ()>(Variadic::from_vec(vec![label
                                    .clone()
                                    .to_value(&self.isolate)
                                    .expect("Failed to convert text edit value")]))
                                .expect("Failed to call input event");
                        }
                    }
                }
            }
        }

        responses
    }
    fn run_microtasks(&self) {
        self.isolate.run_microtasks();
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_element(ui, 0);
            self.run_microtasks();
            // self.print_tree(0, 0);

            // Text editor test
            let mut code = String::new();
            let label = ui.label("Enter code:");
            ui.text_edit_singleline(&mut code).labelled_by(label.id);
        });
    }
}
