use base64::Engine;

use crate::bindings::set_function_property;
use crate::values::v8_str;

pub(crate) fn install_base64(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    set_function_property(scope, global, "atob", atob);
    set_function_property(scope, global, "btoa", btoa);
}

fn atob(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let input = args.get(0).to_rust_string_lossy(scope);
    match base64::engine::general_purpose::STANDARD.decode(input.as_bytes()) {
        Ok(bytes) => {
            let output: String = bytes.into_iter().map(char::from).collect();
            rv.set(v8_str(scope, &output).into());
        }
        Err(error) => {
            let message = v8_str(scope, &error.to_string());
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
        }
    }
}

fn btoa(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let bytes = input.chars().map(|ch| ch as u8).collect::<Vec<_>>();
    let output = base64::engine::general_purpose::STANDARD.encode(bytes);
    rv.set(v8_str(scope, &output).into());
}
