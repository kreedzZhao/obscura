use crate::bindings::{
    accessor, define_template_accessors, instantiate_with_state, AccessorSpec, AccessorValue,
};
use crate::state::NativeState;
use crate::values::{set_property, v8_str};

pub(crate) const NAVIGATOR_ACCESSORS: &[AccessorSpec] = &[
    accessor("navigator", "userAgent", AccessorValue::NavigatorUserAgent),
    accessor(
        "navigator",
        "appVersion",
        AccessorValue::NavigatorAppVersion,
    ),
    accessor("navigator", "platform", AccessorValue::NavigatorPlatform),
    accessor("navigator", "language", AccessorValue::NavigatorLanguage),
    accessor("navigator", "languages", AccessorValue::NavigatorLanguages),
    accessor("navigator", "webdriver", AccessorValue::Bool(false)),
    accessor(
        "navigator",
        "hardwareConcurrency",
        AccessorValue::NavigatorHardwareConcurrency,
    ),
    accessor(
        "navigator",
        "deviceMemory",
        AccessorValue::NavigatorDeviceMemory,
    ),
    accessor("navigator", "cookieEnabled", AccessorValue::Bool(true)),
    accessor("navigator", "maxTouchPoints", AccessorValue::U32(0)),
    accessor("navigator", "vendor", AccessorValue::String("Google Inc.")),
    accessor("navigator", "product", AccessorValue::String("Gecko")),
    accessor("navigator", "productSub", AccessorValue::String("20030107")),
    accessor("navigator", "doNotTrack", AccessorValue::Null),
    accessor("navigator", "pdfViewerEnabled", AccessorValue::Bool(true)),
    accessor("navigator", "onLine", AccessorValue::Bool(true)),
    accessor("navigator", "plugins", AccessorValue::NavigatorPlugins),
    accessor("navigator", "mimeTypes", AccessorValue::NavigatorMimeTypes),
];

pub(crate) fn create_navigator<'s>(
    scope: &mut v8::HandleScope<'s>,
    state_ptr: *mut NativeState,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);
    define_template_accessors(scope, template, NAVIGATOR_ACCESSORS);
    instantiate_with_state(scope, template, state_ptr)
}

pub(crate) fn create_plugin_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    name: &str,
) -> v8::Local<'s, v8::Object> {
    let plugin = v8::Object::new(scope);
    let name_value = v8_str(scope, name);
    set_property(scope, plugin, "name", name_value.into());
    let filename = v8_str(scope, "internal-pdf-viewer");
    set_property(scope, plugin, "filename", filename.into());
    let description = v8_str(scope, "Portable Document Format");
    set_property(scope, plugin, "description", description.into());
    let length = v8::Integer::new(scope, 1);
    set_property(scope, plugin, "length", length.into());
    let tag = v8::String::new(scope, "Plugin").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    plugin.set(scope, key.into(), tag.into());
    plugin
}

pub(crate) fn create_mime_type_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    mime_type: &str,
) -> v8::Local<'s, v8::Object> {
    let object = v8::Object::new(scope);
    let type_value = v8_str(scope, mime_type);
    set_property(scope, object, "type", type_value.into());
    let description = v8_str(scope, "Portable Document Format");
    set_property(scope, object, "description", description.into());
    let suffixes = v8_str(scope, "pdf");
    set_property(scope, object, "suffixes", suffixes.into());
    let enabled_plugin = v8::null(scope);
    set_property(scope, object, "enabledPlugin", enabled_plugin.into());
    let tag = v8::String::new(scope, "MimeType").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    object.set(scope, key.into(), tag.into());
    object
}
