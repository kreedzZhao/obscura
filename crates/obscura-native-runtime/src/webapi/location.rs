use crate::bindings::{
    accessor, define_template_accessors, instantiate_with_state, AccessorSpec, AccessorValue,
};
use crate::state::NativeState;

pub(crate) const LOCATION_ACCESSORS: &[AccessorSpec] =
    &[accessor("location", "href", AccessorValue::LocationHref)];

pub(crate) fn create_location<'s>(
    scope: &mut v8::HandleScope<'s>,
    state_ptr: *mut NativeState,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);
    define_template_accessors(scope, template, LOCATION_ACCESSORS);
    instantiate_with_state(scope, template, state_ptr)
}
