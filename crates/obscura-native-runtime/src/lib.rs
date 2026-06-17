//! Native V8 runtime experiment for Obscura.

use std::sync::Once;

use obscura_dom::{parse_html, DomTree};

mod bindings;
mod state;
mod trace;
mod values;
mod webapi;

use state::NativeState;
use trace::trace_json_from_events;
pub use trace::TraceEvent;
use values::{exception_string, v8_value_to_json};

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
