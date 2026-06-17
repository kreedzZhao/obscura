//! Native V8 runtime experiment for Obscura.

use std::sync::Once;

use base64::Engine;
use obscura_dom::{parse_html, DomTree, NodeData, NodeId};

mod bindings;
mod state;
mod trace;
mod values;
mod webapi;

use bindings::{
    accessor, define_template_accessors, instantiate_with_state, set_function_property,
    set_template_accessor, set_template_function, set_traced_function_property,
    set_traced_template_function, state_ptr, AccessorSpec, AccessorValue,
};
use state::{NativeState, TimerTask};
pub use trace::TraceEvent;
use trace::{record_function_trace, trace_args, trace_json_from_events};
use values::{exception_string, set_property, v8_str, v8_value_to_json};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub url: String,
    pub user_agent: String,
    pub platform: String,
    pub language: String,
    pub languages: Vec<String>,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub color_depth: u32,
    pub window_inner_width: u32,
    pub window_inner_height: u32,
    pub window_outer_width: u32,
    pub window_outer_height: u32,
    pub device_pixel_ratio: f64,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        RuntimeOptions {
            url: "about:blank".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".to_string(),
            platform: "Win32".to_string(),
            language: "en-US".to_string(),
            languages: vec!["en-US".to_string(), "en".to_string()],
            hardware_concurrency: 8,
            device_memory: 8,
            screen_width: 1920,
            screen_height: 1080,
            color_depth: 24,
            window_inner_width: 1920,
            window_inner_height: 1080,
            window_outer_width: 1920,
            window_outer_height: 1080,
            device_pixel_ratio: 1.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("JavaScript compile error: {0}")]
    Compile(String),
    #[error("JavaScript runtime error: {0}")]
    Runtime(String),
    #[error("JavaScript value cannot be represented as JSON: {0}")]
    Json(String),
}

pub struct NativeRuntime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
    state: Box<NativeState>,
}

pub(crate) const DOCUMENT_ACCESSORS: &[AccessorSpec] = &[
    accessor("document", "URL", AccessorValue::DocumentUrl),
    accessor("document", "title", AccessorValue::DocumentTitle),
    accessor("document", "nodeType", AccessorValue::U32(9)),
    AccessorSpec {
        target: "document",
        name: "cookie",
        value: AccessorValue::DocumentCookie,
        writable: true,
    },
];

const CANVAS_2D_NOOP_METHODS: &[&str] = &[
    "fillRect",
    "fillText",
    "strokeText",
    "beginPath",
    "closePath",
    "moveTo",
    "lineTo",
    "arc",
    "rect",
    "fill",
    "stroke",
    "save",
    "restore",
    "translate",
    "rotate",
    "scale",
    "setTransform",
    "resetTransform",
    "transform",
    "clip",
    "setLineDash",
    "ellipse",
    "roundRect",
];

impl NativeRuntime {
    pub fn new(options: RuntimeOptions) -> Self {
        initialize_v8();

        let mut state = Box::new(NativeState {
            title: String::new(),
            dom: DomTree::new(),
            cookies: Vec::new(),
            next_timer_id: 1,
            timers: Vec::new(),
            trace: Vec::new(),
            options,
        });
        let state_ptr = state.as_mut() as *mut NativeState;

        let mut isolate = v8::Isolate::new(Default::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        let context = {
            let scope = &mut v8::HandleScope::new(&mut isolate);
            let global_template = create_global_template(scope);
            let context = v8::Context::new(
                scope,
                v8::ContextOptions {
                    global_template: Some(global_template),
                    ..Default::default()
                },
            );
            let scope = &mut v8::ContextScope::new(scope, context);
            webapi::install_browser_objects(scope, state_ptr);
            v8::Global::new(scope, context)
        };

        NativeRuntime {
            isolate,
            context,
            state,
        }
    }

    pub fn load_html(&mut self, html: &str) -> Result<(), RuntimeError> {
        self.state.dom = parse_html(html);
        self.state.title = self
            .state
            .dom
            .query_selector("title")
            .map_err(RuntimeError::Runtime)?
            .map(|node_id| self.state.dom.text_content(node_id))
            .unwrap_or_default();
        Ok(())
    }

    pub fn eval_json(&mut self, source: &str) -> Result<serde_json::Value, RuntimeError> {
        let result = {
            let scope = &mut v8::HandleScope::new(&mut self.isolate);
            let context = v8::Local::new(scope, &self.context);
            let scope = &mut v8::ContextScope::new(scope, context);
            let try_catch = &mut v8::TryCatch::new(scope);

            let source = v8::String::new(try_catch, source).ok_or_else(|| {
                RuntimeError::Compile("source is not a valid V8 string".to_string())
            })?;
            let script = v8::Script::compile(try_catch, source, None)
                .ok_or_else(|| RuntimeError::Compile(exception_string(try_catch)))?;
            let value = script
                .run(try_catch)
                .ok_or_else(|| RuntimeError::Runtime(exception_string(try_catch)))?;

            v8_value_to_json(try_catch, value)
        };
        self.isolate.perform_microtask_checkpoint();
        self.drain_event_loop()?;
        result
    }

    pub fn drain_event_loop(&mut self) -> Result<(), RuntimeError> {
        self.pump_v8_message_loop();
        self.isolate.perform_microtask_checkpoint();
        let tasks = std::mem::take(&mut self.state.timers);
        if tasks.is_empty() {
            self.pump_v8_message_loop();
            self.isolate.perform_microtask_checkpoint();
            return Ok(());
        }

        let mut repeating_tasks = Vec::new();
        {
            let scope = &mut v8::HandleScope::new(&mut self.isolate);
            let context = v8::Local::new(scope, &self.context);
            let scope = &mut v8::ContextScope::new(scope, context);
            let try_catch = &mut v8::TryCatch::new(scope);
            let receiver = context.global(try_catch);

            for task in tasks {
                let callback = v8::Local::new(try_catch, &task.callback);
                callback
                    .call(try_catch, receiver.into(), &[])
                    .ok_or_else(|| RuntimeError::Runtime(exception_string(try_catch)))?;
                if task.repeat {
                    repeating_tasks.push(task);
                }
            }
        }
        self.state.timers.extend(repeating_tasks);
        self.pump_v8_message_loop();
        self.isolate.perform_microtask_checkpoint();
        Ok(())
    }

    pub fn trace(&self) -> &[TraceEvent] {
        &self.state.trace
    }

    pub fn take_trace(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.state.trace)
    }

    pub fn clear_trace(&mut self) {
        self.state.trace.clear();
    }

    pub fn trace_json(&self) -> serde_json::Value {
        trace_json_from_events(&self.state.trace)
    }

    fn pump_v8_message_loop(&mut self) {
        while v8::Platform::pump_message_loop(
            &v8::V8::get_current_platform(),
            &mut self.isolate,
            false,
        ) {}
    }
}

fn initialize_v8() {
    static START: Once = Once::new();
    START.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

fn create_global_template<'s>(
    scope: &mut v8::HandleScope<'s, ()>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);
    template
}

pub(crate) fn install_dom_constructors(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    webapi::window::install_constructor(
        scope,
        global,
        "HTMLCanvasElement",
        html_canvas_element_constructor,
    );
    webapi::window::install_constructor(
        scope,
        global,
        "CanvasRenderingContext2D",
        canvas_rendering_context_2d_constructor,
    );
    webapi::window::install_constructor(
        scope,
        global,
        "WebGLRenderingContext",
        webgl_rendering_context_constructor,
    );
}

pub(crate) fn install_timer_functions(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    set_function_property(scope, global, "setTimeout", set_timeout);
    set_function_property(scope, global, "setInterval", set_interval);
    set_function_property(scope, global, "clearTimeout", clear_timer);
    set_function_property(scope, global, "clearInterval", clear_timer);
}

pub(crate) fn install_encoding(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    install_encoding_constructor(
        scope,
        global,
        "TextEncoder",
        text_encoder_constructor,
        "encode",
        text_encoder_encode,
    );
    install_encoding_constructor(
        scope,
        global,
        "TextDecoder",
        text_decoder_constructor,
        "decode",
        text_decoder_decode,
    );
}

pub(crate) fn install_base64(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    set_function_property(scope, global, "atob", atob);
    set_function_property(scope, global, "btoa", btoa);
}

pub(crate) fn install_crypto(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let crypto = v8::Object::new(scope);
    set_traced_function_property(
        scope,
        crypto,
        "crypto",
        "getRandomValues",
        crypto_get_random_values,
    );
    set_property(scope, global, "crypto", crypto.into());
}

fn install_encoding_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
    name: &str,
    constructor_callback: impl v8::MapFnTo<v8::FunctionCallback>,
    method_name: &str,
    method_callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let constructor = v8::FunctionTemplate::new(scope, constructor_callback);
    constructor.set_class_name(v8::String::new(scope, name).unwrap());
    let instance = constructor.instance_template(scope);
    set_template_function(scope, instance, method_name, method_callback);
    let function = constructor.get_function(scope).unwrap();
    global.set(scope, key.into(), function.into());
}

pub(crate) fn create_document<'s>(
    scope: &mut v8::HandleScope<'s>,
    state_ptr: *mut NativeState,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);
    define_template_accessors(scope, template, DOCUMENT_ACCESSORS);
    set_traced_template_function(
        scope,
        template,
        "document",
        "querySelector",
        document_query_selector,
    );
    set_traced_template_function(
        scope,
        template,
        "document",
        "createElement",
        document_create_element,
    );

    let document = instantiate_with_state(scope, template, state_ptr);
    let tag = v8::String::new(scope, "Document").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    document.set(scope, key.into(), tag.into());
    document
}

fn create_element<'s>(
    scope: &mut v8::HandleScope<'s>,
    state_ptr: *mut NativeState,
    node_id: NodeId,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(2);
    set_template_accessor(scope, template, "tagName", element_getter);
    set_template_accessor(scope, template, "id", element_getter);
    set_template_accessor(scope, template, "textContent", element_getter);
    set_template_function(scope, template, "getAttribute", element_get_attribute);

    let element = instantiate_with_state(scope, template, state_ptr);
    let node_value = v8::Integer::new_from_unsigned(scope, node_id.raw());
    element.set_internal_field(1, node_value.into());

    let tag = v8::String::new(scope, "Element").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    element.set(scope, key.into(), tag.into());
    element
}

fn html_canvas_element_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let canvas = create_canvas_element(scope);
    rv.set(canvas.into());
}

fn canvas_rendering_context_2d_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = create_canvas_context(scope);
    rv.set(context.into());
}

fn webgl_rendering_context_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = create_webgl_context(scope);
    rv.set(context.into());
}

fn set_timeout(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = allocate_timer(scope, args.get(0), false);
    rv.set(v8::Integer::new_from_unsigned(scope, id).into());
}

fn set_interval(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = allocate_timer(scope, args.get(0), true);
    rv.set(v8::Integer::new_from_unsigned(scope, id).into());
}

fn allocate_timer(
    scope: &mut v8::HandleScope,
    callback_value: v8::Local<v8::Value>,
    repeat: bool,
) -> u32 {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let callback = v8::Local::<v8::Function>::try_from(callback_value)
        .ok()
        .map(|callback| v8::Global::new(scope, callback));
    let state = unsafe { &mut *state_ptr(scope, global) };
    let id = state.next_timer_id;
    state.next_timer_id = state.next_timer_id.saturating_add(1);
    if let Some(callback) = callback {
        state.timers.push(TimerTask {
            id,
            callback,
            repeat,
        });
    }
    id
}

fn clear_timer(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let state = unsafe { &mut *state_ptr(scope, global) };
    let id = args.get(0).uint32_value(scope).unwrap_or(0);
    state.timers.retain(|timer| timer.id != id);
}

fn atob(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let input = args.get(0).to_rust_string_lossy(scope);
    match base64::engine::general_purpose::STANDARD.decode(input.as_bytes()) {
        Ok(bytes) => {
            let output: String = bytes.into_iter().map(char::from).collect();
            rv.set(v8_str(scope, &output).into());
        }
        Err(error) => {
            let message = v8_str(scope, &error.to_string());
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
        }
    }
}

fn btoa(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let bytes = input.chars().map(|ch| ch as u8).collect::<Vec<_>>();
    let output = base64::engine::general_purpose::STANDARD.encode(bytes);
    rv.set(v8_str(scope, &output).into());
}

fn crypto_get_random_values(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let value = args.get(0);
    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
        let message = v8_str(scope, "getRandomValues expects an ArrayBufferView");
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    };

    let length = view.byte_length();
    let data = view.data().cast::<u8>();
    let mut bytes = vec![0; length];
    if let Err(error) = getrandom::getrandom(&mut bytes) {
        let message = v8_str(scope, &error.to_string());
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    }
    for (index, byte) in bytes.into_iter().enumerate() {
        unsafe {
            data.add(index).write(byte);
        }
    }
    record_function_trace(
        scope,
        &args,
        vec![format!("ArrayBufferView({length})")],
        Some(format!("ArrayBufferView({length})")),
    );
    rv.set(value);
}

fn text_encoder_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, "TextEncoder").unwrap();
    if let Some(value) = global.get(scope, key.into()) {
        if let Ok(function) = v8::Local::<v8::Function>::try_from(value) {
            if let Some(instance) = function.new_instance(scope, &[]) {
                rv.set(instance.into());
            }
        }
    }
}

fn text_decoder_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, "TextDecoder").unwrap();
    if let Some(value) = global.get(scope, key.into()) {
        if let Ok(function) = v8::Local::<v8::Function>::try_from(value) {
            if let Some(instance) = function.new_instance(scope, &[]) {
                rv.set(instance.into());
            }
        }
    }
}

fn text_encoder_encode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let bytes = input.into_bytes();
    let length = bytes.len();
    let backing_store = v8::ArrayBuffer::new_backing_store_from_bytes(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    if let Some(array) = v8::Uint8Array::new(scope, buffer, 0, length) {
        rv.set(array.into());
    }
}

fn text_decoder_decode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let value = args.get(0);
    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
        rv.set(v8_str(scope, "").into());
        return;
    };
    let mut bytes = vec![0; view.byte_length()];
    let copied = view.copy_contents(&mut bytes);
    bytes.truncate(copied);
    let decoded = String::from_utf8_lossy(&bytes);
    rv.set(v8_str(scope, decoded.as_ref()).into());
}

fn document_query_selector(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let trace_args = trace_args(scope, &args);
    let state_ptr = state_ptr(scope, args.this());
    let state = unsafe { &mut *state_ptr };
    match state.dom.query_selector(&selector) {
        Ok(Some(node_id)) => {
            let element = create_element(scope, state_ptr, node_id);
            record_function_trace(scope, &args, trace_args, Some("Element".to_string()));
            rv.set(element.into());
        }
        Ok(None) => {
            record_function_trace(scope, &args, trace_args, Some("null".to_string()));
            rv.set(v8::null(scope).into());
        }
        Err(message) => {
            let message = v8_str(scope, &message);
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
        }
    }
}

fn document_create_element(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let tag_name = args.get(0).to_rust_string_lossy(scope);
    let trace_args = trace_args(scope, &args);
    let element = v8::Object::new(scope);
    let tag_name = tag_name.to_ascii_uppercase();
    let tag_value = v8_str(scope, &tag_name);
    set_property(scope, element, "tagName", tag_value.into());
    let node_type = v8::Integer::new(scope, 1);
    set_property(scope, element, "nodeType", node_type.into());

    if tag_name == "CANVAS" {
        install_canvas_members(scope, element);
    }

    let tag = v8::String::new(scope, "Element").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    element.set(scope, key.into(), tag.into());
    record_function_trace(scope, &args, trace_args, Some(tag_name));
    rv.set(element.into());
}

fn create_canvas_element<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let element = v8::Object::new(scope);
    let tag_value = v8_str(scope, "CANVAS");
    set_property(scope, element, "tagName", tag_value.into());
    let node_type = v8::Integer::new(scope, 1);
    set_property(scope, element, "nodeType", node_type.into());
    install_canvas_members(scope, element);
    element
}

fn install_canvas_members(scope: &mut v8::HandleScope, element: v8::Local<v8::Object>) {
    set_traced_function_property(
        scope,
        element,
        "HTMLCanvasElement",
        "getContext",
        canvas_get_context,
    );
    set_traced_function_property(
        scope,
        element,
        "HTMLCanvasElement",
        "toDataURL",
        canvas_to_data_url,
    );

    let global = scope.get_current_context().global(scope);
    let constructor_key = v8::String::new(scope, "HTMLCanvasElement").unwrap();
    if let Some(constructor) = global.get(scope, constructor_key.into()) {
        if let Ok(constructor) = v8::Local::<v8::Function>::try_from(constructor) {
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            if let Some(prototype) = constructor.get(scope, prototype_key.into()) {
                if let Ok(prototype) = v8::Local::<v8::Object>::try_from(prototype) {
                    element.set_prototype(scope, prototype.into());
                }
            }
        }
    }
}

fn canvas_get_context(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let kind = args.get(0).to_rust_string_lossy(scope);
    let trace_args = trace_args(scope, &args);
    if kind.eq_ignore_ascii_case("webgl") || kind.eq_ignore_ascii_case("experimental-webgl") {
        let context = create_webgl_context(scope);
        record_function_trace(
            scope,
            &args,
            trace_args,
            Some("WebGLRenderingContext".to_string()),
        );
        rv.set(context.into());
    } else {
        let context = create_canvas_context(scope);
        record_function_trace(
            scope,
            &args,
            trace_args,
            Some("CanvasRenderingContext2D".to_string()),
        );
        rv.set(context.into());
    }
}

fn create_canvas_context<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let context = v8::Object::new(scope);
    for name in CANVAS_2D_NOOP_METHODS {
        set_traced_function_property(
            scope,
            context,
            "CanvasRenderingContext2D",
            name,
            traced_noop_function,
        );
    }
    let global = scope.get_current_context().global(scope);
    let constructor_key = v8::String::new(scope, "CanvasRenderingContext2D").unwrap();
    if let Some(constructor) = global.get(scope, constructor_key.into()) {
        if let Ok(constructor) = v8::Local::<v8::Function>::try_from(constructor) {
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            if let Some(prototype) = constructor.get(scope, prototype_key.into()) {
                if let Ok(prototype) = v8::Local::<v8::Object>::try_from(prototype) {
                    context.set_prototype(scope, prototype.into());
                }
            }
        }
    }
    context
}

fn create_webgl_context<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let context = v8::Object::new(scope);
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getExtension",
        webgl_get_extension,
    );
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getSupportedExtensions",
        webgl_get_supported_extensions,
    );
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getParameter",
        webgl_get_parameter,
    );
    let global = scope.get_current_context().global(scope);
    let constructor_key = v8::String::new(scope, "WebGLRenderingContext").unwrap();
    if let Some(constructor) = global.get(scope, constructor_key.into()) {
        if let Ok(constructor) = v8::Local::<v8::Function>::try_from(constructor) {
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            if let Some(prototype) = constructor.get(scope, prototype_key.into()) {
                if let Ok(prototype) = v8::Local::<v8::Object>::try_from(prototype) {
                    context.set_prototype(scope, prototype.into());
                }
            }
        }
    }
    context
}

fn webgl_get_extension(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    if name != "WEBGL_debug_renderer_info" {
        record_function_trace(scope, &args, vec![name], Some("null".to_string()));
        rv.set(v8::null(scope).into());
        return;
    }

    let extension = v8::Object::new(scope);
    let vendor = v8::Integer::new_from_unsigned(scope, 0x9245);
    set_property(scope, extension, "UNMASKED_VENDOR_WEBGL", vendor.into());
    let renderer = v8::Integer::new_from_unsigned(scope, 0x9246);
    set_property(scope, extension, "UNMASKED_RENDERER_WEBGL", renderer.into());
    record_function_trace(
        scope,
        &args,
        vec![name],
        Some("WEBGL_debug_renderer_info".to_string()),
    );
    rv.set(extension.into());
}

fn webgl_get_supported_extensions(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let array = v8::Array::new(scope, 0);
    record_function_trace(scope, &args, vec![], Some("Array(0)".to_string()));
    rv.set(array.into());
}

fn webgl_get_parameter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let parameter = args.get(0).uint32_value(scope).unwrap_or(0);
    let result = match parameter {
        0x1f00 | 0x9245 => {
            let value = "Google Inc. (NVIDIA)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x1f01 | 0x9246 => {
            let value =
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 (0x00001F82) Direct3D11 vs_5_0 ps_5_0, D3D11)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x1f02 => {
            let value = "WebGL 1.0 (OpenGL ES 2.0 Chromium)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x8b8c => {
            let value = "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        _ => {
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some("0".to_string()),
            );
            v8::Integer::new(scope, 0).into()
        }
    };
    rv.set(result);
}

fn traced_noop_function(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let trace_args = trace_args(scope, &args);
    record_function_trace(scope, &args, trace_args, None);
}

fn canvas_to_data_url(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    record_function_trace(scope, &args, vec![], Some(String::new()));
    rv.set(v8_str(scope, "").into());
}

fn element_getter(
    scope: &mut v8::HandleScope,
    key: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let element = args.this();
    let state = unsafe { &mut *state_ptr(scope, element) };
    let node_id = element_node_id(scope, element);
    let key = key.to_string(scope).unwrap().to_rust_string_lossy(scope);

    match key.as_str() {
        "tagName" => {
            if let Some(tag_name) = element_tag_name(state, node_id) {
                rv.set(v8_str(scope, &tag_name).into());
            }
        }
        "id" => {
            let id = element_attribute(state, node_id, "id").unwrap_or_default();
            rv.set(v8_str(scope, &id).into());
        }
        "textContent" => {
            let text = state.dom.text_content(node_id);
            rv.set(v8_str(scope, &text).into());
        }
        _ => {}
    }
}

fn element_get_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let element = args.this();
    let state = unsafe { &mut *state_ptr(scope, element) };
    let node_id = element_node_id(scope, element);

    match element_attribute(state, node_id, &name) {
        Some(value) => rv.set(v8_str(scope, &value).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn element_node_id(scope: &mut v8::HandleScope, object: v8::Local<v8::Object>) -> NodeId {
    let value: v8::Local<v8::Value> = object
        .get_internal_field(scope, 1)
        .unwrap()
        .try_into()
        .unwrap();
    NodeId::new(value.uint32_value(scope).unwrap())
}

fn element_tag_name(state: &NativeState, node_id: NodeId) -> Option<String> {
    state.dom.with_node(node_id, |node| match &node.data {
        NodeData::Element { name, .. } => Some(name.local.as_ref().to_ascii_uppercase()),
        _ => None,
    })?
}

fn element_attribute(state: &NativeState, node_id: NodeId, name: &str) -> Option<String> {
    state.dom.with_node(node_id, |node| {
        node.get_attribute(name).map(ToOwned::to_owned)
    })?
}

pub(crate) fn document_cookie(state: &NativeState) -> String {
    state
        .cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

pub(crate) fn set_document_cookie(state: &mut NativeState, cookie: &str) {
    let Some(pair) = cookie.split(';').next() else {
        return;
    };
    let Some((name, value)) = pair.split_once('=') else {
        return;
    };
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let value = value.trim();
    if let Some((_, existing)) = state.cookies.iter_mut().find(|(item, _)| item == name) {
        *existing = value.to_string();
    } else {
        state.cookies.push((name.to_string(), value.to_string()));
    }
}
