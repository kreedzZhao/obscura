use crate::bindings::{
    accessor, define_template_accessors, instantiate_with_state, AccessorSpec, AccessorValue,
};
use crate::state::NativeState;

pub(crate) const SCREEN_ACCESSORS: &[AccessorSpec] = &[
    accessor("screen", "width", AccessorValue::ScreenWidth),
    accessor("screen", "height", AccessorValue::ScreenHeight),
    accessor("screen", "availWidth", AccessorValue::ScreenAvailWidth),
    accessor("screen", "availHeight", AccessorValue::ScreenAvailHeight),
    accessor("screen", "colorDepth", AccessorValue::ScreenColorDepth),
    accessor("screen", "pixelDepth", AccessorValue::ScreenColorDepth),
];

pub(crate) fn create_screen<'s>(
    scope: &mut v8::HandleScope<'s>,
    state_ptr: *mut NativeState,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);
    define_template_accessors(scope, template, SCREEN_ACCESSORS);
    instantiate_with_state(scope, template, state_ptr)
}
