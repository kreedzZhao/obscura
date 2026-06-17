use crate::bindings::set_traced_function_property;
use crate::trace::record_function_trace;
use crate::values::{set_property, v8_str};

pub(crate) fn install_crypto(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let crypto = v8::Object::new(scope);
    set_traced_function_property(
        scope,
        crypto,
        "crypto",
        "getRandomValues",
        crypto_get_random_values,
    );
    set_property(scope, global, "crypto", crypto.into());
}

fn crypto_get_random_values(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let value = args.get(0);
    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
        let message = v8_str(scope, "getRandomValues expects an ArrayBufferView");
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    };

    let length = view.byte_length();
    let data = view.data().cast::<u8>();
    let mut bytes = vec![0; length];
    if let Err(error) = getrandom::getrandom(&mut bytes) {
        let message = v8_str(scope, &error.to_string());
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    }
    for (index, byte) in bytes.into_iter().enumerate() {
        unsafe {
            data.add(index).write(byte);
        }
    }
    record_function_trace(
        scope,
        &args,
        vec![format!("ArrayBufferView({length})")],
        Some(format!("ArrayBufferView({length})")),
    );
    rv.set(value);
}
