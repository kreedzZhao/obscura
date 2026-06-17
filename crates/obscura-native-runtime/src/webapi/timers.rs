use crate::bindings::{set_function_property, state_ptr};
use crate::state::TimerTask;

pub(crate) fn install_timer_functions(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    set_function_property(scope, global, "setTimeout", set_timeout);
    set_function_property(scope, global, "setInterval", set_interval);
    set_function_property(scope, global, "clearTimeout", clear_timer);
    set_function_property(scope, global, "clearInterval", clear_timer);
}

fn set_timeout(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = allocate_timer(scope, args.get(0), false);
    rv.set(v8::Integer::new_from_unsigned(scope, id).into());
}

fn set_interval(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = allocate_timer(scope, args.get(0), true);
    rv.set(v8::Integer::new_from_unsigned(scope, id).into());
}

fn allocate_timer(
    scope: &mut v8::HandleScope,
    callback_value: v8::Local<v8::Value>,
    repeat: bool,
) -> u32 {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let callback = v8::Local::<v8::Function>::try_from(callback_value)
        .ok()
        .map(|callback| v8::Global::new(scope, callback));
    let state = unsafe { &mut *state_ptr(scope, global) };
    let id = state.next_timer_id;
    state.next_timer_id = state.next_timer_id.saturating_add(1);
    if let Some(callback) = callback {
        state.timers.push(TimerTask {
            id,
            callback,
            repeat,
        });
    }
    id
}

fn clear_timer(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let state = unsafe { &mut *state_ptr(scope, global) };
    let id = args.get(0).uint32_value(scope).unwrap_or(0);
    state.timers.retain(|timer| timer.id != id);
}
