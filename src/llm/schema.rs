use serde_json::Value;

/// Transpiles a JSON Schema emitted by `schemars` into a strict Gemini-compatible OpenAPI subset.
pub fn transpile_schema(mut schema: Value) -> Value {
    let defs = schema.get("$defs").cloned();
    transpile_recursive(&mut schema, &defs, 0);

    if let Value::Object(ref mut map) = schema {
        map.remove("$schema");
        map.remove("$defs");
    }

    schema
}

fn transpile_recursive(val: &mut Value, defs: &Option<Value>, depth: usize) {
    if depth > 250 {
        panic!(
            "Acyclic verification failed: Schema traversal exceeded maximum depth safely catching cyclic reference!"
        );
    }

    match val {
        Value::Object(map) => {
            // 1. Resolve $ref top-down
            if let Some(ref_val) = map.get("$ref") {
                if let Some(ref_str) = ref_val.as_str() {
                    if ref_str.starts_with("#/$defs/") {
                        let def_name = ref_str.trim_start_matches("#/$defs/");
                        if let Some(Value::Object(defs_map)) = defs {
                            if let Some(def_schema) = defs_map.get(def_name) {
                                *val = def_schema.clone();
                                transpile_recursive(val, defs, depth + 1);
                                return;
                            }
                        }
                    }
                }
            }

            // 2. Recursively transpile children schema nodes structurally
            if let Some(Value::Object(props)) = map.get_mut("properties") {
                for (_, v) in props.iter_mut() {
                    transpile_recursive(v, defs, depth + 1);
                }
            }
            if let Some(items) = map.get_mut("items") {
                if items.is_object() {
                    transpile_recursive(items, defs, depth + 1);
                } else if let Value::Array(arr) = items {
                    for v in arr.iter_mut() {
                        transpile_recursive(v, defs, depth + 1);
                    }
                }
            }
            if let Some(Value::Array(arr)) = map.get_mut("anyOf") {
                for v in arr.iter_mut() {
                    transpile_recursive(v, defs, depth + 1);
                }
            }
            if let Some(Value::Array(arr)) = map.get_mut("allOf") {
                for v in arr.iter_mut() {
                    transpile_recursive(v, defs, depth + 1);
                }
            }

            // 3. Process anyOf arrays
            if let Some(Value::Array(arr)) = map.remove("anyOf") {
                let mut primary = None;
                let mut nullable = false;
                for item in arr {
                    if item.get("type").and_then(|t| t.as_str()) == Some("null") {
                        nullable = true;
                    } else {
                        primary = Some(item);
                    }
                }

                if let Some(Value::Object(prim_map)) = primary {
                    for (k, v) in prim_map {
                        map.insert(k, v);
                    }
                    if nullable {
                        map.insert("nullable".to_string(), Value::Bool(true));
                    }
                } else if let Some(p) = primary {
                    if nullable {
                        map.insert("nullable".to_string(), Value::Bool(true));
                    }
                    map.insert("anyOf_fallback".to_string(), p);
                }
            }

            // 4. Process allOf combinations
            if let Some(Value::Array(arr)) = map.remove("allOf") {
                let mut merged_props = serde_json::Map::new();
                let mut merged_required = Vec::new();

                for item in arr {
                    if let Value::Object(mut item_map) = item {
                        if let Some(Value::Object(props)) = item_map.remove("properties") {
                            merged_props.extend(props);
                        }
                        if let Some(Value::Array(reqs)) = item_map.remove("required") {
                            merged_required.extend(reqs);
                        }

                        for (k, v) in item_map {
                            if k != "properties" && k != "required" {
                                map.insert(k, v);
                            }
                        }
                    }
                }

                if !merged_props.is_empty() {
                    let map_props = map
                        .entry("properties".to_string())
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    if let Value::Object(existing_props) = map_props {
                        existing_props.extend(merged_props);
                    }
                }

                if !merged_required.is_empty() {
                    let map_reqs = map
                        .entry("required".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(existing_reqs) = map_reqs {
                        existing_reqs.extend(merged_required);
                    }
                }
            }

            // 5. Handle type array combinations
            if let Some(type_val) = map.get_mut("type") {
                if let Value::Array(arr) = type_val {
                    let mut primary_type = None;
                    let mut is_nullable = false;
                    for item in arr.iter() {
                        if item.as_str() == Some("null") {
                            is_nullable = true;
                        } else {
                            primary_type = Some(item.clone());
                        }
                    }
                    if let Some(pt) = primary_type {
                        *type_val = pt;
                    }
                    if is_nullable {
                        map.insert("nullable".to_string(), Value::Bool(true));
                    }
                }
            }

            // 6. Cleanup `required` arrays optionally
            if let Some(Value::Array(mut reqs)) = map.remove("required") {
                if let Some(Value::Object(props)) = map.get("properties") {
                    reqs.retain(|r| {
                        if let Some(k) = r.as_str() {
                            if let Some(Value::Object(prop_map)) = props.get(k) {
                                if prop_map.get("nullable") == Some(&Value::Bool(true)) {
                                    return false; // optional cleanly explicitly
                                }
                            }
                        }
                        true
                    });
                }
                if !reqs.is_empty() {
                    map.insert("required".to_string(), Value::Array(reqs));
                }
            }

            // 7. Prune unsupported nodes gracefully rigidly elegantly smoothly
            let valid_keys = [
                "type",
                "description",
                "properties",
                "required",
                "items",
                "enum",
                "nullable",
            ];
            map.retain(|k, _| valid_keys.contains(&k.as_str()));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_transpile_strips_metadata() {
        let input = json!({
            "$schema": "foo",
            "$defs": { "A": { "type": "string" } },
            "type": "string",
            "title": "Unwanted",
            "default": "value",
            "additionalProperties": false,
            "format": "date-time"
        });
        let res = transpile_schema(input);
        assert!(res.get("$schema").is_none());
        assert!(res.get("$defs").is_none());
        assert!(res.get("title").is_none());
        assert!(res.get("default").is_none());
        assert!(res.get("additionalProperties").is_none());
        assert!(res.get("format").is_none());
        assert_eq!(res["type"], "string");
    }

    #[test]
    fn test_transpile_resolves_ref() {
        let input = json!({
            "$defs": { "A": { "type": "string" } },
            "properties": {
                "val": { "$ref": "#/$defs/A" }
            }
        });
        let res = transpile_schema(input);
        assert_eq!(res["properties"]["val"]["type"], "string");
        assert!(res["properties"]["val"].get("$ref").is_none());
    }

    #[test]
    fn test_transpile_handles_any_of_nullability() {
        let input = json!({
            "properties": {
                "val": {
                    "anyOf": [
                        { "type": "string", "description": "some doc" },
                        { "type": "null" }
                    ]
                }
            }
        });
        let res = transpile_schema(input);
        assert_eq!(res["properties"]["val"]["type"], "string");
        assert_eq!(res["properties"]["val"]["description"], "some doc");
        assert_eq!(res["properties"]["val"]["nullable"], true);
        assert!(res["properties"]["val"].get("anyOf").is_none());
    }

    #[test]
    fn test_transpile_handles_all_of_merging() {
        let input = json!({
            "allOf": [
                { "type": "object", "properties": { "field_a": { "type": "string" } }, "required": ["field_a"] },
                { "type": "object", "properties": { "field_b": { "type": "integer" } }, "required": ["field_b"] }
            ]
        });
        let res = transpile_schema(input);
        assert_eq!(res["type"], "object");
        assert_eq!(res["properties"]["field_a"]["type"], "string");
        assert_eq!(res["properties"]["field_b"]["type"], "integer");
        assert_eq!(res["required"][0], "field_a");
        assert_eq!(res["required"][1], "field_b");
        assert!(res.get("allOf").is_none());
    }

    #[test]
    fn test_transpile_cleans_required_nullables() {
        let input = json!({
            "type": "object",
            "properties": {
                "req": { "type": "string" },
                "opt": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                }
            },
            "required": ["req", "opt"]
        });

        let res = transpile_schema(input);

        assert_eq!(res["properties"]["opt"]["nullable"], true);
        assert_eq!(res["required"].as_array().unwrap().len(), 1);
        assert_eq!(res["required"][0], "req");
    }

    #[test]
    fn test_transpile_handles_type_array_nullable() {
        let input = json!({
            "type": ["string", "null"]
        });
        let res = transpile_schema(input);
        assert_eq!(res["type"], "string");
        assert_eq!(res["nullable"], true);
    }

    #[test]
    fn test_transpile_handles_type_array_non_nullable() {
        let input = json!({
            "type": ["integer"]
        });
        let res = transpile_schema(input);
        assert_eq!(res["type"], "integer");
        assert!(res.get("nullable").is_none());
    }

    #[test]
    fn test_transpile_deep_ref_and_flattening() {
        let input = json!({
            "$defs": {
                "A": {
                    "type": "object",
                    "properties": { "b": { "$ref": "#/$defs/B" } }
                },
                "B": {
                    "anyOf": [ { "type": "string" }, { "type": "null" } ]
                }
            },
            "$ref": "#/$defs/A"
        });
        let res = transpile_schema(input);
        assert_eq!(res["type"], "object");
        assert_eq!(res["properties"]["b"]["type"], "string");
        assert_eq!(res["properties"]["b"]["nullable"], true);
    }

    #[test]
    fn test_transpile_array_items() {
        let input = json!({
            "type": "array",
            "items": [
                { "type": ["boolean", "null"] },
                { "title": "del", "type": "integer" }
            ]
        });
        let res = transpile_schema(input);
        assert_eq!(res["items"][0]["type"], "boolean");
        assert_eq!(res["items"][0]["nullable"], true);
        assert!(res["items"][1].get("title").is_none());
        assert_eq!(res["items"][1]["type"], "integer");
    }

    #[test]
    #[should_panic(
        expected = "Acyclic verification failed: Schema traversal exceeded maximum depth safely catching cyclic reference!"
    )]
    fn test_transpile_cyclic_ref_panics_explicitly() {
        let input = json!({
            "$defs": {
                "A": { "$ref": "#/$defs/B" },
                "B": { "$ref": "#/$defs/A" }
            },
            "$ref": "#/$defs/A"
        });
        let _ = transpile_schema(input);
    }

    #[test]
    fn test_transpile_handles_objects_nested_deeply() {
        let mut nested = json!({"type": "string"});
        for _ in 0..50 {
            nested = json!({"type": "object", "properties": {"v": nested}});
        }
        let res = transpile_schema(nested);
        assert_eq!(res["type"], "object");
    }
}
