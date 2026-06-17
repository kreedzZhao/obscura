pub(crate) mod base64;
pub(crate) mod crypto;
pub(crate) mod document;
pub(crate) mod encoding;
pub(crate) mod location;
pub(crate) mod navigator;
pub(crate) mod screen;
pub(crate) mod timers;
pub(crate) mod window;

use std::ffi::c_void;

use crate::state::NativeState;
use crate::values::set_property;

pub(crate) fn install_browser_objects(scope: &mut v8::HandleScope, state_ptr: *mut NativeState) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let external = v8::External::new(scope, state_ptr.cast::<c_void>());
    global.set_internal_field(0, external.into());

    window::install_window_constructor(scope, global);
    crate::install_dom_constructors(scope, global);
    set_property(scope, global, "window", global.into());
    set_property(scope, global, "self", global.into());
    window::install_window_values(scope, global);
    timers::install_timer_functions(scope, global);
    encoding::install_encoding(scope, global);
    base64::install_base64(scope, global);
    crypto::install_crypto(scope, global);

    let navigator = navigator::create_navigator(scope, state_ptr);
    set_property(scope, global, "navigator", navigator.into());

    let screen = screen::create_screen(scope, state_ptr);
    set_property(scope, global, "screen", screen.into());

    let location = location::create_location(scope, state_ptr);
    set_property(scope, global, "location", location.into());

    let document = document::create_document(scope, state_ptr);
    set_property(scope, global, "document", document.into());
}
