use crate::bindings::set_traced_function_property;
use crate::trace::record_function_trace;
use crate::values::{set_property, v8_str};

pub(crate) fn install_webgl_constructor(
    scope: &mut v8::HandleScope,
    global: v8::Local<v8::Object>,
) {
    crate::webapi::window::install_constructor(
        scope,
        global,
        "WebGLRenderingContext",
        webgl_rendering_context_constructor,
    );
}

fn webgl_rendering_context_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.is_construct_call() {
        rv.set(args.this().into());
        return;
    }
    let context = create_webgl_context(scope);
    rv.set(context.into());
}

pub(crate) fn create_webgl_context<'s>(
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let context = v8::Object::new(scope);
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getExtension",
        webgl_get_extension,
    );
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getSupportedExtensions",
        webgl_get_supported_extensions,
    );
    set_traced_function_property(
        scope,
        context,
        "WebGLRenderingContext",
        "getParameter",
        webgl_get_parameter,
    );
    let global = scope.get_current_context().global(scope);
    let constructor_key = v8::String::new(scope, "WebGLRenderingContext").unwrap();
    if let Some(constructor) = global.get(scope, constructor_key.into()) {
        if let Ok(constructor) = v8::Local::<v8::Function>::try_from(constructor) {
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            if let Some(prototype) = constructor.get(scope, prototype_key.into()) {
                if let Ok(prototype) = v8::Local::<v8::Object>::try_from(prototype) {
                    context.set_prototype(scope, prototype.into());
                }
            }
        }
    }
    context
}

fn webgl_get_extension(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    if name != "WEBGL_debug_renderer_info" {
        record_function_trace(scope, &args, vec![name], Some("null".to_string()));
        rv.set(v8::null(scope).into());
        return;
    }

    let extension = v8::Object::new(scope);
    let vendor = v8::Integer::new_from_unsigned(scope, 0x9245);
    set_property(scope, extension, "UNMASKED_VENDOR_WEBGL", vendor.into());
    let renderer = v8::Integer::new_from_unsigned(scope, 0x9246);
    set_property(scope, extension, "UNMASKED_RENDERER_WEBGL", renderer.into());
    record_function_trace(
        scope,
        &args,
        vec![name],
        Some("WEBGL_debug_renderer_info".to_string()),
    );
    rv.set(extension.into());
}

fn webgl_get_supported_extensions(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let array = v8::Array::new(scope, 0);
    record_function_trace(scope, &args, vec![], Some("Array(0)".to_string()));
    rv.set(array.into());
}

fn webgl_get_parameter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let parameter = args.get(0).uint32_value(scope).unwrap_or(0);
    let result = match parameter {
        0x1f00 | 0x9245 => {
            let value = "Google Inc. (NVIDIA)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x1f01 | 0x9246 => {
            let value =
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 (0x00001F82) Direct3D11 vs_5_0 ps_5_0, D3D11)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x1f02 => {
            let value = "WebGL 1.0 (OpenGL ES 2.0 Chromium)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        0x8b8c => {
            let value = "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)";
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some(value.to_string()),
            );
            v8_str(scope, value).into()
        }
        _ => {
            record_function_trace(
                scope,
                &args,
                vec![parameter.to_string()],
                Some("0".to_string()),
            );
            v8::Integer::new(scope, 0).into()
        }
    };
    rv.set(result);
}
