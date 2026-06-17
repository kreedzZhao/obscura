//! Native V8 runtime experiment for Obscura.

use std::sync::Once;

use obscura_dom::{parse_html, DomTree};

mod bindings;
mod state;
mod trace;
mod values;
mod webapi;

use bindings::set_traced_function_property;
use state::NativeState;
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

fn create_canvas_element<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let element = v8::Object::new(scope);
    let tag_value = v8_str(scope, "CANVAS");
    set_property(scope, element, "tagName", tag_value.into());
    let node_type = v8::Integer::new(scope, 1);
    set_property(scope, element, "nodeType", node_type.into());
    install_canvas_members(scope, element);
    element
}

pub(crate) fn install_canvas_members(scope: &mut v8::HandleScope, element: v8::Local<v8::Object>) {
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
