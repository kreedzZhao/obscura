use crate::bindings::{accessor, define_object_accessors, AccessorSpec, AccessorValue};

pub(crate) const WINDOW_ACCESSORS: &[AccessorSpec] = &[
    accessor(
        "window",
        "devicePixelRatio",
        AccessorValue::WindowDevicePixelRatio,
    ),
    accessor("window", "innerWidth", AccessorValue::WindowInnerWidth),
    accessor("window", "innerHeight", AccessorValue::WindowInnerHeight),
    accessor("window", "outerWidth", AccessorValue::WindowOuterWidth),
    accessor("window", "outerHeight", AccessorValue::WindowOuterHeight),
];

pub(crate) fn install_window_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
) {
    let key = v8::String::new(scope, "Window").unwrap();
    let template = v8::FunctionTemplate::new(scope, window_constructor);
    template.set_class_name(key);
    let function = template.get_function(scope).unwrap();
    global.set(scope, key.into(), function.into());

    let prototype_key = v8::String::new(scope, "prototype").unwrap();
    if let Some(prototype) = function.get(scope, prototype_key.into()) {
        if let Ok(prototype) = v8::Local::<v8::Object>::try_from(prototype) {
            global.set_prototype(scope, prototype.into());
        }
    }
}

pub(crate) fn install_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let template = v8::FunctionTemplate::new(scope, callback);
    template.set_class_name(key);
    let function = template.get_function(scope).unwrap();
    global.set(scope, key.into(), function.into());
}

pub(crate) fn install_window_values(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    define_object_accessors(scope, global, WINDOW_ACCESSORS);
}

fn window_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let global = scope.get_current_context().global(scope);
    rv.set(global.into());
}
