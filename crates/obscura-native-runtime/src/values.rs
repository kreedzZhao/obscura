use crate::RuntimeError;

pub(crate) fn set_property(
    scope: &mut v8::HandleScope,
    object: v8::Local<v8::Object>,
    key: &str,
    value: v8::Local<v8::Value>,
) {
    let key = v8::String::new(scope, key).unwrap();
    object.set(scope, key.into(), value);
}

pub(crate) fn v8_str<'s>(
    scope: &mut v8::HandleScope<'s>,
    value: &str,
) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, value).unwrap()
}

pub(crate) fn v8_value_to_json(
    scope: &mut v8::HandleScope,
    value: v8::Local<v8::Value>,
) -> Result<serde_json::Value, RuntimeError> {
    if value.is_undefined() {
        return Ok(serde_json::Value::Null);
    }

    let global = scope.get_current_context().global(scope);
    let json_key = v8::String::new(scope, "JSON").unwrap();
    let json = global.get(scope, json_key.into()).unwrap();
    let json = v8::Local::<v8::Object>::try_from(json)
        .map_err(|_| RuntimeError::Json("global JSON is not an object".to_string()))?;
    let stringify_key = v8::String::new(scope, "stringify").unwrap();
    let stringify = json.get(scope, stringify_key.into()).unwrap();
    let stringify = v8::Local::<v8::Function>::try_from(stringify)
        .map_err(|_| RuntimeError::Json("JSON.stringify is not a function".to_string()))?;
    let json_value = stringify
        .call(scope, json.into(), &[value])
        .ok_or_else(|| RuntimeError::Json("JSON.stringify failed".to_string()))?;

    if json_value.is_undefined() {
        return Ok(serde_json::Value::Null);
    }

    let json_string = json_value
        .to_string(scope)
        .ok_or_else(|| RuntimeError::Json("JSON.stringify did not return a string".to_string()))?
        .to_rust_string_lossy(scope);
    serde_json::from_str(&json_string).map_err(|err| RuntimeError::Json(err.to_string()))
}

pub(crate) fn exception_string<P>(_scope: &mut v8::TryCatch<P>) -> String {
    "unknown exception".to_string()
}
