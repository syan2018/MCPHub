use serde_json::{Map, Number, Value};

pub fn coerce_value_for_path(root_schema: Option<&Value>, path: &[&str], raw: &str) -> Value {
    match root_schema.and_then(|schema| schema_for_path(schema, path)) {
        Some(schema) => {
            coerce_value_from_schema(schema, raw).unwrap_or_else(|| parse_inline_value(raw))
        }
        None => parse_inline_value(raw),
    }
}

pub fn build_input_template(schema: &Value) -> Value {
    build_template_value(schema).unwrap_or_else(|| Value::Object(Map::new()))
}

pub fn build_default_input(schema: &Value) -> Value {
    build_default_value(schema).unwrap_or_else(|| Value::Object(Map::new()))
}

fn build_template_value(schema: &Value) -> Option<Value> {
    if let Some(suggested) = build_suggested_value(schema) {
        return Some(suggested);
    }

    if let Some(result) =
        build_object_like_value(schema, build_template_value, build_suggested_value, true)
    {
        return Some(result);
    }

    let primary_type = schema_type_names(schema)
        .into_iter()
        .find(|item| *item != "null");
    match primary_type.as_deref() {
        Some("string") => Some(Value::String("<string>".into())),
        Some("integer") => Some(Value::Number(Number::from(0))),
        Some("number") => Some(Value::Number(Number::from_f64(0.0)?)),
        Some("boolean") => Some(Value::Bool(false)),
        Some("array") => Some(Value::Array(Vec::new())),
        Some("object") => Some(Value::Object(Map::new())),
        _ => first_composed_schema(schema).and_then(build_template_value),
    }
}

fn build_default_value(schema: &Value) -> Option<Value> {
    if let Some(default) = schema.get("default") {
        return Some(default.clone());
    }

    if let Some(result) =
        build_object_like_value(schema, build_default_value, build_default_value, false)
    {
        return Some(result);
    }

    first_composed_schema(schema).and_then(build_default_value)
}

fn build_object_like_value(
    schema: &Value,
    required_value: fn(&Value) -> Option<Value>,
    optional_value: fn(&Value) -> Option<Value>,
    include_required_without_default: bool,
) -> Option<Value> {
    let properties = merged_properties(schema);
    if properties.is_empty() {
        return None;
    }

    let required = merged_required(schema);
    let mut result = Map::new();
    for (key, property_schema) in properties {
        let value = if let Some(suggested) = optional_value(&property_schema) {
            Some(suggested)
        } else if include_required_without_default && required.contains(&key) {
            required_value(&property_schema)
        } else {
            None
        };
        if let Some(value) = value {
            result.insert(key, value);
        }
    }
    Some(Value::Object(result))
}

fn schema_for_path<'a>(schema: &'a Value, path: &[&str]) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(schema);
    }

    let (first, rest) = path.split_first()?;
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        if let Some(next) = properties.get(*first) {
            return schema_for_path(next, rest);
        }
    }

    for nested in composed_schemas(schema) {
        if let Some(found) = schema_for_path(nested, path) {
            return Some(found);
        }
    }

    None
}

fn coerce_value_from_schema(schema: &Value, raw: &str) -> Option<Value> {
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if let Some(value) = match_enum_value(enum_values, raw) {
            return Some(value);
        }
    }

    let types = schema_type_names(schema);

    if types.iter().any(|item| *item == "string") {
        return Some(Value::String(raw.to_string()));
    }

    if types.iter().any(|item| *item == "boolean") {
        if raw.eq_ignore_ascii_case("true") {
            return Some(Value::Bool(true));
        }
        if raw.eq_ignore_ascii_case("false") {
            return Some(Value::Bool(false));
        }
    }

    if types.iter().any(|item| *item == "integer") {
        if let Ok(value) = raw.parse::<i64>() {
            return Some(Value::Number(Number::from(value)));
        }
        if let Ok(value) = raw.parse::<u64>() {
            return Some(Value::Number(Number::from(value)));
        }
    }

    if types.iter().any(|item| *item == "number") {
        if let Ok(value) = raw.parse::<f64>() {
            if let Some(number) = Number::from_f64(value) {
                return Some(Value::Number(number));
            }
        }
    }

    if types.iter().any(|item| *item == "null") && raw.eq_ignore_ascii_case("null") {
        return Some(Value::Null);
    }

    if types
        .iter()
        .any(|item| *item == "array" || *item == "object")
    {
        if let Ok(value) = serde_json::from_str::<Value>(raw) {
            let kind = value_kind(&value);
            if types.iter().any(|item| *item == kind) {
                return Some(value);
            }
        }
    }

    for nested in composed_schemas(schema) {
        if let Some(value) = coerce_value_from_schema(nested, raw) {
            return Some(value);
        }
    }

    None
}

fn match_enum_value(enum_values: &[Value], raw: &str) -> Option<Value> {
    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        if let Some(matched) = enum_values.iter().find(|value| **value == parsed) {
            return Some(matched.clone());
        }
    }

    enum_values.iter().find_map(|value| match value {
        Value::String(text) if text == raw => Some(Value::String(text.clone())),
        Value::Bool(flag) if raw.eq_ignore_ascii_case(if *flag { "true" } else { "false" }) => {
            Some(Value::Bool(*flag))
        }
        Value::Number(number) if number.to_string() == raw => Some(Value::Number(number.clone())),
        _ => None,
    })
}

fn schema_type_names(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn merged_properties(schema: &Value) -> Map<String, Value> {
    let mut result = Map::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, value) in properties {
            result.insert(key.clone(), value.clone());
        }
    }

    for nested in composed_schemas(schema) {
        for (key, value) in merged_properties(nested) {
            result.entry(key).or_insert(value);
        }
    }

    result
}

fn merged_required(schema: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for item in required.iter().filter_map(Value::as_str) {
            let item = item.to_string();
            if !result.contains(&item) {
                result.push(item);
            }
        }
    }

    for nested in composed_schemas(schema) {
        for item in merged_required(nested) {
            if !result.contains(&item) {
                result.push(item);
            }
        }
    }

    result
}

fn build_suggested_value(schema: &Value) -> Option<Value> {
    if let Some(default) = schema.get("default") {
        return Some(default.clone());
    }

    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if let Some(first) = enum_values.first() {
            return Some(first.clone());
        }
    }

    for nested in composed_schemas(schema) {
        if let Some(value) = build_suggested_value(nested) {
            return Some(value);
        }
    }

    None
}

fn first_composed_schema(schema: &Value) -> Option<&Value> {
    composed_schemas(schema).into_iter().next()
}

fn composed_schemas(schema: &Value) -> Vec<&Value> {
    let mut result = Vec::new();
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            for item in items {
                result.push(item);
            }
        }
    }
    result
}

fn parse_inline_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_default_input, build_input_template, coerce_value_for_path};

    #[test]
    fn coerce_value_uses_schema_string_type() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        });
        assert_eq!(
            coerce_value_for_path(Some(&schema), &["query"], "123"),
            json!("123")
        );
    }

    #[test]
    fn coerce_value_uses_nested_schema_types() {
        let schema = json!({
            "type": "object",
            "properties": {
                "arguments": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer" }
                    }
                }
            }
        });
        assert_eq!(
            coerce_value_for_path(Some(&schema), &["arguments", "limit"], "42"),
            json!(42)
        );
    }

    #[test]
    fn build_input_template_keeps_required_fields() {
        let schema = json!({
            "type": "object",
            "properties": {
                "libraryName": { "type": "string" },
                "query": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["libraryName", "query"]
        });

        assert_eq!(
            build_input_template(&schema),
            json!({
                "libraryName": "<string>",
                "query": "<string>"
            })
        );
    }

    #[test]
    fn build_input_template_prefers_default_and_enum() {
        let schema = json!({
            "type": "object",
            "properties": {
                "mode": { "enum": ["fast", "safe"] },
                "limit": { "type": "integer", "default": 10 }
            }
        });

        assert_eq!(
            build_input_template(&schema),
            json!({
                "mode": "fast",
                "limit": 10
            })
        );
    }

    #[test]
    fn build_default_input_uses_defaults_only() {
        let schema = json!({
            "type": "object",
            "properties": {
                "mode": { "enum": ["fast", "safe"] },
                "limit": { "type": "integer", "default": 10 }
            },
            "required": ["mode", "limit"]
        });

        assert_eq!(build_default_input(&schema), json!({ "limit": 10 }));
    }

    #[test]
    fn composed_schema_supports_path_lookup_and_defaults() {
        let schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                },
                {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "default": 5 }
                    }
                }
            ]
        });

        assert_eq!(
            coerce_value_for_path(Some(&schema), &["query"], "abc"),
            json!("abc")
        );
        assert_eq!(
            build_default_input(&schema),
            json!({
                "limit": 5
            })
        );
        assert_eq!(
            build_input_template(&schema),
            json!({
                "query": "<string>",
                "limit": 5
            })
        );
    }
}
