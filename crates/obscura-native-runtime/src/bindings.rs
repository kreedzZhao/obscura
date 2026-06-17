use std::ffi::c_void;

use crate::state::NativeState;
use crate::trace::record_trace;
use crate::values::v8_str;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AccessorSpec {
    pub(crate) target: &'static str,
    pub(crate) name: &'static str,
    pub(crate) value: AccessorValue,
    pub(crate) writable: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AccessorValue {
    NavigatorUserAgent,
    NavigatorAppVersion,
    NavigatorPlatform,
    NavigatorLanguage,
    NavigatorLanguages,
    NavigatorHardwareConcurrency,
    NavigatorDeviceMemory,
    NavigatorPlugins,
    NavigatorMimeTypes,
    LocationHref,
    DocumentUrl,
    DocumentTitle,
    DocumentCookie,
    ScreenWidth,
    ScreenHeight,
    ScreenAvailWidth,
    ScreenAvailHeight,
    ScreenColorDepth,
    WindowDevicePixelRatio,
    WindowInnerWidth,
    WindowInnerHeight,
    WindowOuterWidth,
    WindowOuterHeight,
    String(&'static str),
    Bool(bool),
    Null,
    U32(u32),
}

pub(crate) const fn accessor(
    target: &'static str,
    name: &'static str,
    value: AccessorValue,
) -> AccessorSpec {
    AccessorSpec {
        target,
        name,
        value,
        writable: false,
    }
}

pub(crate) fn instantiate_with_state<'s>(
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    state_ptr: *mut NativeState,
) -> v8::Local<'s, v8::Object> {
    let object = template.new_instance(scope).unwrap();
    let external = v8::External::new(scope, state_ptr.cast::<c_void>());
    object.set_internal_field(0, external.into());
    object
}

pub(crate) fn set_template_accessor(
    scope: &mut v8::HandleScope,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    template.set_accessor(key.into(), getter);
}

pub(crate) fn define_template_accessors(
    scope: &mut v8::HandleScope,
    template: v8::Local<v8::ObjectTemplate>,
    specs: &[AccessorSpec],
) {
    for spec in specs {
        let key = v8::String::new(scope, spec.name).unwrap();
        let data = binding_data(scope, spec.target, spec.name);
        let mut config = v8::AccessorConfiguration::new(webapi_getter).data(data);
        if spec.writable {
            config = config.setter(webapi_setter);
        }
        template.set_accessor_with_configuration(key.into(), config);
    }
}

pub(crate) fn define_object_accessors(
    scope: &mut v8::HandleScope,
    object: v8::Local<v8::Object>,
    specs: &[AccessorSpec],
) {
    for spec in specs {
        let key = v8::String::new(scope, spec.name).unwrap();
        let data = binding_data(scope, spec.target, spec.name);
        let mut config = v8::AccessorConfiguration::new(webapi_getter).data(data);
        if spec.writable {
            config = config.setter(webapi_setter);
        }
        object.set_accessor_with_configuration(scope, key.into(), config);
    }
}

pub(crate) fn set_template_function(
    scope: &mut v8::HandleScope,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let function = v8::FunctionTemplate::new(scope, callback);
    template.set(key.into(), function.into());
}

pub(crate) fn set_traced_template_function(
    scope: &mut v8::HandleScope,
    template: v8::Local<v8::ObjectTemplate>,
    target: &str,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let data = binding_data(scope, target, name);
    let function = v8::FunctionTemplate::builder(callback)
        .data(data)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    template.set(key.into(), function.into());
}

pub(crate) fn set_function_property(
    scope: &mut v8::HandleScope,
    object: v8::Local<v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let template = v8::FunctionTemplate::new(scope, callback);
    let function = template.get_function(scope).unwrap();
    object.set(scope, key.into(), function.into());
}

pub(crate) fn set_traced_function_property(
    scope: &mut v8::HandleScope,
    object: v8::Local<v8::Object>,
    target: &str,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let data = binding_data(scope, target, name);
    let template = v8::FunctionTemplate::builder(callback)
        .data(data)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    let function = template.get_function(scope).unwrap();
    object.set(scope, key.into(), function.into());
}

fn binding_data<'s>(
    scope: &mut v8::HandleScope<'s>,
    target: &str,
    name: &str,
) -> v8::Local<'s, v8::Value> {
    v8_str(scope, &format!("{target}\u{1f}{name}")).into()
}

pub(crate) fn binding_from_data(
    scope: &mut v8::HandleScope,
    data: v8::Local<v8::Value>,
) -> Option<(String, String)> {
    let data = data.to_string(scope)?.to_rust_string_lossy(scope);
    let (target, name) = data.split_once('\u{1f}')?;
    Some((target.to_string(), name.to_string()))
}

fn find_accessor_spec(target: &str, name: &str) -> Option<&'static AccessorSpec> {
    [
        crate::webapi::window::WINDOW_ACCESSORS,
        crate::webapi::navigator::NAVIGATOR_ACCESSORS,
        crate::webapi::screen::SCREEN_ACCESSORS,
        crate::webapi::location::LOCATION_ACCESSORS,
        crate::webapi::document::DOCUMENT_ACCESSORS,
    ]
    .into_iter()
    .flat_map(|specs| specs.iter())
    .find(|spec| spec.target == target && spec.name == name)
}

fn webapi_getter(
    scope: &mut v8::HandleScope,
    key: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let key = key.to_string(scope).unwrap().to_rust_string_lossy(scope);
    let (target, name) = binding_from_data(scope, args.data())
        .unwrap_or_else(|| ("unknown".to_string(), key.clone()));
    let Some(spec) = find_accessor_spec(&target, &name) else {
        record_trace(scope, &target, &name, "get-miss", vec![], None);
        return;
    };

    let state = unsafe { &*state_ptr(scope, args.holder()) };
    let (value, result) = accessor_value_to_v8(scope, state, spec.value);
    record_trace(scope, spec.target, spec.name, "get", vec![], Some(result));
    rv.set(value);
}

fn webapi_setter(
    scope: &mut v8::HandleScope,
    key: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _rv: v8::ReturnValue<()>,
) {
    let key = key.to_string(scope).unwrap().to_rust_string_lossy(scope);
    let (target, name) = binding_from_data(scope, args.data())
        .unwrap_or_else(|| ("unknown".to_string(), key.clone()));
    let value = value.to_rust_string_lossy(scope);
    record_trace(
        scope,
        &target,
        &name,
        "set",
        vec![value.clone()],
        Some(value.clone()),
    );

    if let ("document", "cookie") = (target.as_str(), name.as_str()) {
        let state = unsafe { &mut *state_ptr(scope, args.holder()) };
        crate::webapi::document::set_document_cookie(state, &value);
    }
}

fn accessor_value_to_v8<'s>(
    scope: &mut v8::HandleScope<'s>,
    state: &NativeState,
    value: AccessorValue,
) -> (v8::Local<'s, v8::Value>, String) {
    match value {
        AccessorValue::NavigatorUserAgent => {
            let value = state.options.user_agent.clone();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::NavigatorAppVersion => {
            let value = state
                .options
                .user_agent
                .strip_prefix("Mozilla/")
                .unwrap_or(&state.options.user_agent)
                .to_string();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::NavigatorPlatform => {
            let value = state.options.platform.clone();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::NavigatorLanguage => {
            let value = state.options.language.clone();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::NavigatorLanguages => {
            let values = state.options.languages.clone();
            let array = v8::Array::new(scope, values.len() as i32);
            for (index, value) in values.iter().enumerate() {
                let value = v8_str(scope, value);
                array.set_index(scope, index as u32, value.into());
            }
            (array.into(), format!("{values:?}"))
        }
        AccessorValue::NavigatorHardwareConcurrency => {
            let value = state.options.hardware_concurrency;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::NavigatorDeviceMemory => {
            let value = state.options.device_memory;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::NavigatorPlugins => {
            let plugins = [
                "PDF Viewer",
                "Chrome PDF Viewer",
                "Chromium PDF Viewer",
                "Microsoft Edge PDF Viewer",
                "WebKit built-in PDF",
            ];
            let array = v8::Array::new(scope, plugins.len() as i32);
            for (index, name) in plugins.iter().enumerate() {
                let plugin = crate::webapi::navigator::create_plugin_object(scope, name);
                array.set_index(scope, index as u32, plugin.into());
            }
            (array.into(), "Array(5)".to_string())
        }
        AccessorValue::NavigatorMimeTypes => {
            let mime_types = ["application/pdf", "text/pdf"];
            let array = v8::Array::new(scope, mime_types.len() as i32);
            for (index, name) in mime_types.iter().enumerate() {
                let mime_type = crate::webapi::navigator::create_mime_type_object(scope, name);
                array.set_index(scope, index as u32, mime_type.into());
            }
            (array.into(), "Array(2)".to_string())
        }
        AccessorValue::LocationHref | AccessorValue::DocumentUrl => {
            let value = state.options.url.clone();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::DocumentTitle => {
            let value = state.title.clone();
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::DocumentCookie => {
            let value = crate::webapi::document::document_cookie(state);
            (v8_str(scope, &value).into(), value)
        }
        AccessorValue::ScreenWidth => {
            let value = state.options.screen_width;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::ScreenHeight => {
            let value = state.options.screen_height;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::ScreenAvailWidth => {
            let value = state.options.screen_width;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::ScreenAvailHeight => {
            let value = state.options.screen_height.saturating_sub(40);
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::ScreenColorDepth => {
            let value = state.options.color_depth;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::WindowDevicePixelRatio => {
            let value = state.options.device_pixel_ratio;
            (v8::Number::new(scope, value).into(), value.to_string())
        }
        AccessorValue::WindowInnerWidth => {
            let value = state.options.window_inner_width;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::WindowInnerHeight => {
            let value = state.options.window_inner_height;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::WindowOuterWidth => {
            let value = state.options.window_outer_width;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::WindowOuterHeight => {
            let value = state.options.window_outer_height;
            (
                v8::Integer::new_from_unsigned(scope, value).into(),
                value.to_string(),
            )
        }
        AccessorValue::String(value) => (v8_str(scope, value).into(), value.to_string()),
        AccessorValue::Bool(value) => (v8::Boolean::new(scope, value).into(), value.to_string()),
        AccessorValue::Null => (v8::null(scope).into(), "null".to_string()),
        AccessorValue::U32(value) => (
            v8::Integer::new_from_unsigned(scope, value).into(),
            value.to_string(),
        ),
    }
}

pub(crate) fn state_ptr(
    scope: &mut v8::HandleScope,
    object: v8::Local<v8::Object>,
) -> *mut NativeState {
    let external = object
        .get_internal_field(scope, 0)
        .unwrap()
        .cast::<v8::External>();
    external.value() as *mut NativeState
}
