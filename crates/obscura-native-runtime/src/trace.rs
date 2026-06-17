#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub target: String,
    pub name: String,
    pub kind: String,
    pub args: Vec<String>,
    pub result: Option<String>,
}

pub(crate) fn trace_json_from_events(events: &[TraceEvent]) -> serde_json::Value {
    serde_json::Value::Array(
        events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "target": event.target,
                    "name": event.name,
                    "kind": event.kind,
                    "args": event.args,
                    "result": event.result,
                })
            })
            .collect(),
    )
}

pub(crate) fn record_trace(
    scope: &mut v8::HandleScope,
    target: &str,
    name: &str,
    kind: &str,
    args: Vec<String>,
    result: Option<String>,
) {
    let global = scope.get_current_context().global(scope);
    let state = unsafe { &mut *crate::state_ptr(scope, global) };
    state.trace.push(TraceEvent {
        target: target.to_string(),
        name: name.to_string(),
        kind: kind.to_string(),
        args,
        result,
    });
}

pub(crate) fn record_function_trace(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    call_args: Vec<String>,
    result: Option<String>,
) {
    let (target, name) = crate::binding_from_data(scope, args.data())
        .unwrap_or_else(|| ("unknown".to_string(), "anonymous".to_string()));
    record_trace(scope, &target, &name, "call", call_args, result);
}

pub(crate) fn trace_args(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
) -> Vec<String> {
    let mut values = Vec::new();
    for index in 0..args.length() {
        values.push(trace_value(scope, args.get(index)));
    }
    values
}

pub(crate) fn trace_value(scope: &mut v8::HandleScope, value: v8::Local<v8::Value>) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if value.is_undefined() {
        return "undefined".to_string();
    }
    if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) {
        return format!("ArrayBufferView({})", view.byte_length());
    }
    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_else(|| "[unprintable]".to_string())
}
