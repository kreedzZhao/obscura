use crate::bindings::set_template_function;
use crate::values::v8_str;

pub(crate) fn install_encoding(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    install_encoding_constructor(
        scope,
        global,
        "TextEncoder",
        text_encoder_constructor,
        "encode",
        text_encoder_encode,
    );
    install_encoding_constructor(
        scope,
        global,
        "TextDecoder",
        text_decoder_constructor,
        "decode",
        text_decoder_decode,
    );
}

fn install_encoding_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
    name: &str,
    constructor_callback: impl v8::MapFnTo<v8::FunctionCallback>,
    method_name: &str,
    method_callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let constructor = v8::FunctionTemplate::new(scope, constructor_callback);
    constructor.set_class_name(v8::String::new(scope, name).unwrap());
    let instance = constructor.instance_template(scope);
    set_template_function(scope, instance, method_name, method_callback);
    let function = constructor.get_function(scope).unwrap();
    global.set(scope, key.into(), function.into());
}

fn text_encoder_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, "TextEncoder").unwrap();
    if let Some(value) = global.get(scope, key.into()) {
        if let Ok(function) = v8::Local::<v8::Function>::try_from(value) {
            if let Some(instance) = function.new_instance(scope, &[]) {
                rv.set(instance.into());
            }
        }
    }
}

fn text_decoder_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, "TextDecoder").unwrap();
    if let Some(value) = global.get(scope, key.into()) {
        if let Ok(function) = v8::Local::<v8::Function>::try_from(value) {
            if let Some(instance) = function.new_instance(scope, &[]) {
                rv.set(instance.into());
            }
        }
    }
}

fn text_encoder_encode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let bytes = input.into_bytes();
    let length = bytes.len();
    let backing_store = v8::ArrayBuffer::new_backing_store_from_bytes(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    if let Some(array) = v8::Uint8Array::new(scope, buffer, 0, length) {
        rv.set(array.into());
    }
}

fn text_decoder_decode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let value = args.get(0);
    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
        rv.set(v8_str(scope, "").into());
        return;
    };
    let mut bytes = vec![0; view.byte_length()];
    let copied = view.copy_contents(&mut bytes);
    bytes.truncate(copied);
    let decoded = String::from_utf8_lossy(&bytes);
    rv.set(v8_str(scope, decoded.as_ref()).into());
}
