use crate::bindings::set_traced_function_property;
use crate::trace::{record_function_trace, trace_args};
use crate::values::{set_property, v8_str};

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

pub(crate) fn install_canvas_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
) {
    crate::webapi::window::install_constructor(
        scope,
        global,
        "HTMLCanvasElement",
        html_canvas_element_constructor,
    );
    crate::webapi::window::install_constructor(
        scope,
        global,
        "CanvasRenderingContext2D",
        canvas_rendering_context_2d_constructor,
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

pub(crate) fn create_canvas_element<'s>(
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
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
        let context = crate::webapi::webgl::create_webgl_context(scope);
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
