use obscura_dom::{NodeData, NodeId};

use crate::bindings::{
    accessor, define_template_accessors, instantiate_with_state, set_template_accessor,
    set_template_function, set_traced_template_function, state_ptr, AccessorSpec, AccessorValue,
};
use crate::state::NativeState;
use crate::trace::{record_function_trace, trace_args};
use crate::values::{set_property, v8_str};

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
        crate::webapi::canvas::install_canvas_members(scope, element);
    }

    let tag = v8::String::new(scope, "Element").unwrap();
    let key = v8::Symbol::get_to_string_tag(scope);
    element.set(scope, key.into(), tag.into());
    record_function_trace(scope, &args, trace_args, Some(tag_name));
    rv.set(element.into());
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
