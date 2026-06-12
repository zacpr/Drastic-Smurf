use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type PipelineDocument = Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProcessorType {
    #[default]
    Set,
    Remove,
    Json,
    Reroute,
    Convert,
    Lowercase,
    Uppercase,
    Trim,
}

impl ProcessorType {
    pub const ALL: &[ProcessorType] = &[
        ProcessorType::Set,
        ProcessorType::Remove,
        ProcessorType::Json,
        ProcessorType::Reroute,
        ProcessorType::Convert,
        ProcessorType::Lowercase,
        ProcessorType::Uppercase,
        ProcessorType::Trim,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessorType::Set => "set",
            ProcessorType::Remove => "remove",
            ProcessorType::Json => "json",
            ProcessorType::Reroute => "reroute",
            ProcessorType::Convert => "convert",
            ProcessorType::Lowercase => "lowercase",
            ProcessorType::Uppercase => "uppercase",
            ProcessorType::Trim => "trim",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvertType {
    String,
    Integer,
    Float,
    Boolean,
}

impl ConvertType {
    pub const ALL: &[ConvertType] = &[
        ConvertType::String,
        ConvertType::Integer,
        ConvertType::Float,
        ConvertType::Boolean,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ConvertType::String => "string",
            ConvertType::Integer => "integer",
            ConvertType::Float => "float",
            ConvertType::Boolean => "boolean",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Processor {
    Set {
        id: String,
        field: String,
        value: Value,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Remove {
        id: String,
        fields: Vec<String>,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Json {
        id: String,
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_field: Option<String>,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Reroute {
        id: String,
        dataset: String,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Convert {
        id: String,
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_field: Option<String>,
        convert_to: ConvertType,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Lowercase {
        id: String,
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_field: Option<String>,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Uppercase {
        id: String,
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_field: Option<String>,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
    Trim {
        id: String,
        field: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_field: Option<String>,
        #[serde(default)]
        ignore_failure: bool,
        #[serde(default)]
        if_condition: Option<String>,
    },
}

impl Processor {
    pub fn id(&self) -> &str {
        match self {
            Processor::Set { id, .. } => id,
            Processor::Remove { id, .. } => id,
            Processor::Json { id, .. } => id,
            Processor::Reroute { id, .. } => id,
            Processor::Convert { id, .. } => id,
            Processor::Lowercase { id, .. } => id,
            Processor::Uppercase { id, .. } => id,
            Processor::Trim { id, .. } => id,
        }
    }

    pub fn processor_type(&self) -> ProcessorType {
        match self {
            Processor::Set { .. } => ProcessorType::Set,
            Processor::Remove { .. } => ProcessorType::Remove,
            Processor::Json { .. } => ProcessorType::Json,
            Processor::Reroute { .. } => ProcessorType::Reroute,
            Processor::Convert { .. } => ProcessorType::Convert,
            Processor::Lowercase { .. } => ProcessorType::Lowercase,
            Processor::Uppercase { .. } => ProcessorType::Uppercase,
            Processor::Trim { .. } => ProcessorType::Trim,
        }
    }

    pub fn ignore_failure(&self) -> bool {
        match self {
            Processor::Set { ignore_failure, .. } => *ignore_failure,
            Processor::Remove { ignore_failure, .. } => *ignore_failure,
            Processor::Json { ignore_failure, .. } => *ignore_failure,
            Processor::Reroute { ignore_failure, .. } => *ignore_failure,
            Processor::Convert { ignore_failure, .. } => *ignore_failure,
            Processor::Lowercase { ignore_failure, .. } => *ignore_failure,
            Processor::Uppercase { ignore_failure, .. } => *ignore_failure,
            Processor::Trim { ignore_failure, .. } => *ignore_failure,
        }
    }

    pub fn set_ignore_failure(&mut self, val: bool) {
        match self {
            Processor::Set { ignore_failure, .. } => *ignore_failure = val,
            Processor::Remove { ignore_failure, .. } => *ignore_failure = val,
            Processor::Json { ignore_failure, .. } => *ignore_failure = val,
            Processor::Reroute { ignore_failure, .. } => *ignore_failure = val,
            Processor::Convert { ignore_failure, .. } => *ignore_failure = val,
            Processor::Lowercase { ignore_failure, .. } => *ignore_failure = val,
            Processor::Uppercase { ignore_failure, .. } => *ignore_failure = val,
            Processor::Trim { ignore_failure, .. } => *ignore_failure = val,
        }
    }

    pub fn if_condition(&self) -> Option<&str> {
        match self {
            Processor::Set { if_condition, .. } => if_condition.as_deref(),
            Processor::Remove { if_condition, .. } => if_condition.as_deref(),
            Processor::Json { if_condition, .. } => if_condition.as_deref(),
            Processor::Reroute { if_condition, .. } => if_condition.as_deref(),
            Processor::Convert { if_condition, .. } => if_condition.as_deref(),
            Processor::Lowercase { if_condition, .. } => if_condition.as_deref(),
            Processor::Uppercase { if_condition, .. } => if_condition.as_deref(),
            Processor::Trim { if_condition, .. } => if_condition.as_deref(),
        }
    }

    pub fn set_if_condition(&mut self, cond: Option<String>) {
        match self {
            Processor::Set { if_condition, .. } => *if_condition = cond,
            Processor::Remove { if_condition, .. } => *if_condition = cond,
            Processor::Json { if_condition, .. } => *if_condition = cond,
            Processor::Reroute { if_condition, .. } => *if_condition = cond,
            Processor::Convert { if_condition, .. } => *if_condition = cond,
            Processor::Lowercase { if_condition, .. } => *if_condition = cond,
            Processor::Uppercase { if_condition, .. } => *if_condition = cond,
            Processor::Trim { if_condition, .. } => *if_condition = cond,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub processor_id: String,
    pub processor_type: ProcessorType,
    pub before: PipelineDocument,
    pub after: PipelineDocument,
    pub changed_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub ignored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub steps: Vec<TraceStep>,
    pub final_document: PipelineDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------- Path helpers ----------

pub fn split_path(path: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut in_bracket = false;
    let mut in_quote: Option<char> = None;
    let mut escaping = false;

    let chars = path.chars().peekable();

    for ch in chars {
        if let Some(quote) = in_quote {
            if escaping {
                buffer.push(ch);
                escaping = false;
                continue;
            }
            if ch == '\\' {
                escaping = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
                continue;
            }
            buffer.push(ch);
            continue;
        }

        if in_bracket {
            if ch == '"' || ch == '\'' {
                in_quote = Some(ch);
                continue;
            }
            if ch == ']' {
                let trimmed = buffer.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
                buffer.clear();
                in_bracket = false;
                continue;
            }
            if ch.is_whitespace() {
                continue;
            }
            buffer.push(ch);
            continue;
        }

        if ch == '.' {
            if !buffer.is_empty() {
                parts.push(buffer.trim().to_string());
                buffer.clear();
            }
            continue;
        }

        if ch == '[' {
            if !buffer.is_empty() {
                parts.push(buffer.trim().to_string());
                buffer.clear();
            }
            in_bracket = true;
            continue;
        }

        buffer.push(ch);
    }

    if in_quote.is_some() || in_bracket {
        return Err(format!("invalid field path: \"{}\"", path));
    }

    if !buffer.is_empty() {
        parts.push(buffer.trim().to_string());
    }

    Ok(parts)
}

fn is_object(value: &Value) -> bool {
    value.is_object()
}

pub fn get_by_path<'v>(document: &'v PipelineDocument, path: &str) -> Option<&'v Value> {
    let parts = match split_path(path) {
        Ok(p) => p,
        Err(_) => return None,
    };
    if parts.is_empty() {
        return None;
    }

    let mut cursor: &Value = document;
    for key in &parts {
        match cursor {
            Value::Object(map) => {
                cursor = map.get(key)?;
            }
            _ => return None,
        }
    }
    Some(cursor)
}

pub fn set_by_path(document: &PipelineDocument, path: &str, value: Value) -> Result<Value, String> {
    let parts = split_path(path)?;
    if parts.is_empty() {
        return Err("set processor requires a non-empty field path".to_string());
    }

    let mut next = document.clone();
    let mut cursor = &mut next;

    for key in &parts[..parts.len() - 1] {
        match cursor {
            Value::Object(map) => {
                if !map.get(key).map(is_object).unwrap_or(false) {
                    map.insert(key.clone(), Value::Object(serde_json::Map::new()));
                }
                cursor = map.get_mut(key).unwrap();
            }
            _ => return Err(format!("cannot navigate path at \"{}\"", key)),
        }
    }

    match cursor {
        Value::Object(map) => {
            map.insert(parts.last().unwrap().clone(), value);
        }
        _ => return Err("target is not an object".to_string()),
    }

    Ok(next)
}

pub fn remove_by_path(document: &PipelineDocument, path: &str) -> Result<Value, String> {
    let parts = split_path(path)?;
    if parts.is_empty() {
        return Ok(document.clone());
    }

    let mut next = document.clone();
    let mut cursor: *mut Value = &mut next;

    for key in &parts[..parts.len() - 1] {
        unsafe {
            match &mut *cursor {
                Value::Object(map) => {
                    if !map.get(key).map(|v| v.is_object()).unwrap_or(false) {
                        return Ok(next);
                    }
                    cursor = map.get_mut(key).unwrap() as *mut Value;
                }
                _ => return Ok(next),
            }
        }
    }

    unsafe {
        if let Value::Object(map) = &mut *cursor {
            map.remove(parts.last().unwrap());
        }
    }

    Ok(next)
}

// ---------- Deep diff ----------

pub fn collect_changed_paths(before: &Value, after: &Value, base_path: &str) -> Vec<String> {
    match (before, after) {
        (Value::Object(before_map), Value::Object(after_map)) => {
            let mut changed = Vec::new();
            let keys: std::collections::HashSet<_> =
                before_map.keys().chain(after_map.keys()).cloned().collect();

            for key in keys {
                let child_path = if base_path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", base_path, key)
                };

                match (before_map.get(&key), after_map.get(&key)) {
                    (Some(b), Some(a)) => {
                        changed.extend(collect_changed_paths(b, a, &child_path));
                    }
                    _ => {
                        changed.push(child_path);
                    }
                }
            }
            changed
        }
        _ => {
            if before == after {
                Vec::new()
            } else {
                vec![if base_path.is_empty() {
                    "$".to_string()
                } else {
                    base_path.to_string()
                }]
            }
        }
    }
}

// ---------- Painless Condition Evaluator ----------

pub fn evaluate_if_condition(document: &Value, condition: &str) -> Result<bool, String> {
    let cond = condition.trim();
    if cond.is_empty() {
        return Ok(true);
    }

    let mut expr = cond;
    if expr.starts_with("if") {
        expr = expr["if".len()..].trim();
    }
    if expr.starts_with('(') && expr.ends_with(')') {
        expr = expr[1..expr.len() - 1].trim();
    }

    if expr.contains("==") {
        let parts: Vec<&str> = expr.split("==").collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return Ok(left == right);
        }
    } else if expr.contains("!=") {
        let parts: Vec<&str> = expr.split("!=").collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return Ok(left != right);
        }
    } else if expr.contains(">=") {
        let parts: Vec<&str> = expr.split(">=").collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return compare_numeric_vals(&left, &right, |a, b| a >= b);
        }
    } else if expr.contains("<=") {
        let parts: Vec<&str> = expr.split("<=").collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return compare_numeric_vals(&left, &right, |a, b| a <= b);
        }
    } else if expr.contains('>') {
        let parts: Vec<&str> = expr.split('>').collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return compare_numeric_vals(&left, &right, |a, b| a > b);
        }
    } else if expr.contains('<') {
        let parts: Vec<&str> = expr.split('<').collect();
        if parts.len() == 2 {
            let left = get_ctx_val(document, parts[0].trim())?;
            let right = parse_literal(parts[1].trim());
            return compare_numeric_vals(&left, &right, |a, b| a < b);
        }
    } else if expr.contains(".contains(") {
        if let Some(idx) = expr.find(".contains(") {
            let field_path = &expr[..idx].trim();
            let start_arg = idx + ".contains(".len();
            if let Some(end_idx) = expr[start_arg..].find(')') {
                let arg = &expr[start_arg..start_arg + end_idx].trim();
                let left_val = get_ctx_val(document, field_path)?;
                let left_str = match &left_val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let right_str = match parse_literal(arg) {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                return Ok(left_str.contains(&right_str));
            }
        }
    } else if expr.contains(".startsWith(") {
        if let Some(idx) = expr.find(".startsWith(") {
            let field_path = &expr[..idx].trim();
            let start_arg = idx + ".startsWith(".len();
            if let Some(end_idx) = expr[start_arg..].find(')') {
                let arg = &expr[start_arg..start_arg + end_idx].trim();
                let left_val = get_ctx_val(document, field_path)?;
                let left_str = match &left_val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let right_str = match parse_literal(arg) {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                return Ok(left_str.starts_with(&right_str));
            }
        }
    } else if expr.contains(".endsWith(") {
        if let Some(idx) = expr.find(".endsWith(") {
            let field_path = &expr[..idx].trim();
            let start_arg = idx + ".endsWith(".len();
            if let Some(end_idx) = expr[start_arg..].find(')') {
                let arg = &expr[start_arg..start_arg + end_idx].trim();
                let left_val = get_ctx_val(document, field_path)?;
                let left_str = match &left_val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let right_str = match parse_literal(arg) {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                return Ok(left_str.ends_with(&right_str));
            }
        }
    } else {
        if let Ok(val) = get_ctx_val(document, expr) {
            return Ok(val.as_bool().unwrap_or(!val.is_null()));
        }
    }

    Err(format!("could not parse painless condition: \"{}\"", expr))
}

fn get_ctx_val(document: &Value, path: &str) -> Result<Value, String> {
    let mut clean_path = path.trim().to_string();
    let is_safe = clean_path.contains('?');

    if clean_path.starts_with("ctx?.") {
        clean_path = clean_path["ctx?.".len()..].to_string();
    } else if clean_path.starts_with("ctx.") {
        clean_path = clean_path["ctx.".len()..].to_string();
    }

    let normalized_path = clean_path.replace('?', "");

    match get_by_path(document, &normalized_path) {
        Some(val) => Ok(val.clone()),
        None => {
            if is_safe {
                Ok(Value::Null)
            } else {
                Err(format!(
                    "field \"{}\" not found in document",
                    normalized_path
                ))
            }
        }
    }
}

fn parse_literal(val: &str) -> Value {
    let clean = val.trim();
    if (clean.starts_with('"') && clean.ends_with('"'))
        || (clean.starts_with('\'') && clean.ends_with('\''))
    {
        return Value::String(clean[1..clean.len() - 1].to_string());
    }
    if clean == "true" {
        return Value::Bool(true);
    }
    if clean == "false" {
        return Value::Bool(false);
    }
    if clean == "null" {
        return Value::Null;
    }
    if let Ok(i) = clean.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Ok(f) = clean.parse::<f64>()
        && let Some(num) = serde_json::Number::from_f64(f)
    {
        return Value::Number(num);
    }
    Value::String(clean.to_string())
}

fn compare_numeric_vals<F>(left: &Value, right: &Value, op: F) -> Result<bool, String>
where
    F: Fn(f64, f64) -> bool,
{
    let l = left.as_f64().or_else(|| left.as_i64().map(|i| i as f64));
    let r = right.as_f64().or_else(|| right.as_i64().map(|i| i as f64));
    match (l, r) {
        (Some(la), Some(ra)) => Ok(op(la, ra)),
        _ => Err(format!(
            "cannot compare non-numeric values: {:?} and {:?}",
            left, right
        )),
    }
}

// ---------- Processor application ----------

fn apply_set(document: &Value, field: &str, value: &Value) -> Result<Value, String> {
    set_by_path(document, field, value.clone())
}

fn apply_remove(document: &Value, fields: &[String]) -> Result<Value, String> {
    let mut current = document.clone();
    for field in fields {
        current = remove_by_path(&current, field)?;
    }
    Ok(current)
}

fn apply_json(document: &Value, field: &str, target_field: Option<&str>) -> Result<Value, String> {
    let value = get_by_path(document, field)
        .ok_or_else(|| format!("json processor: field \"{}\" not found", field))?;
    let s = value
        .as_str()
        .ok_or_else(|| format!("json processor expected string at \"{}\"", field))?;
    let parsed: Value = serde_json::from_str(s)
        .map_err(|e| format!("json parse failed at \"{}\": {}", field, e))?;
    set_by_path(document, target_field.unwrap_or(field), parsed)
}

fn apply_reroute(document: &Value, dataset: &str) -> Result<Value, String> {
    let mut next = set_by_path(
        document,
        "_ingest.reroute.dataset",
        Value::String(dataset.to_string()),
    )?;
    next = set_by_path(
        &next,
        "_ingest.reroute.target_index",
        Value::String(format!("logs-{}-default", dataset)),
    )?;
    Ok(next)
}

fn value_to_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

fn convert_value(value: &Value, convert_to: ConvertType) -> Result<Value, String> {
    match convert_to {
        ConvertType::String => Ok(Value::String(value_to_str(value))),
        ConvertType::Integer => {
            let s = value_to_str(value);
            let n = s
                .parse::<i64>()
                .map_err(|_| format!("cannot convert \"{}\" to integer", s))?;
            Ok(Value::Number(n.into()))
        }
        ConvertType::Float => {
            let s = value_to_str(value);
            let n = s
                .parse::<f64>()
                .map_err(|_| format!("cannot convert \"{}\" to float", s))?;
            let num = serde_json::Number::from_f64(n)
                .ok_or_else(|| format!("cannot convert \"{}\" to float", s))?;
            Ok(Value::Number(num))
        }
        ConvertType::Boolean => {
            if let Some(b) = value.as_bool() {
                return Ok(Value::Bool(b));
            }
            let s = value_to_str(value);
            let normalized = s.trim().to_lowercase();
            if normalized == "true" || normalized == "1" {
                Ok(Value::Bool(true))
            } else if normalized == "false" || normalized == "0" {
                Ok(Value::Bool(false))
            } else {
                Err(format!("cannot convert \"{}\" to boolean", s))
            }
        }
    }
}

fn apply_convert(
    document: &Value,
    field: &str,
    target_field: Option<&str>,
    convert_to: ConvertType,
) -> Result<Value, String> {
    let source = get_by_path(document, field)
        .ok_or_else(|| format!("convert processor source field not found: \"{}\"", field))?;
    let converted = convert_value(source, convert_to)?;
    set_by_path(document, target_field.unwrap_or(field), converted)
}

fn apply_string_transform(
    document: &Value,
    field: &str,
    target_field: Option<&str>,
    transform: ProcessorType,
) -> Result<Value, String> {
    let source = get_by_path(document, field).ok_or_else(|| {
        format!(
            "{} processor expected string at \"{}\"",
            transform.as_str(),
            field
        )
    })?;
    let s = source.as_str().ok_or_else(|| {
        format!(
            "{} processor expected string at \"{}\"",
            transform.as_str(),
            field
        )
    })?;
    let result = match transform {
        ProcessorType::Lowercase => s.to_lowercase(),
        ProcessorType::Uppercase => s.to_uppercase(),
        ProcessorType::Trim => s.trim().to_string(),
        _ => unreachable!(),
    };
    set_by_path(
        document,
        target_field.unwrap_or(field),
        Value::String(result),
    )
}

pub fn apply_processor(document: &Value, processor: &Processor) -> Result<Value, String> {
    match processor {
        Processor::Set { field, value, .. } => apply_set(document, field, value),
        Processor::Remove { fields, .. } => apply_remove(document, fields),
        Processor::Json {
            field,
            target_field,
            ..
        } => apply_json(document, field, target_field.as_deref()),
        Processor::Reroute { dataset, .. } => apply_reroute(document, dataset),
        Processor::Convert {
            field,
            target_field,
            convert_to,
            ..
        } => apply_convert(document, field, target_field.as_deref(), *convert_to),
        Processor::Lowercase {
            field,
            target_field,
            ..
        } => apply_string_transform(
            document,
            field,
            target_field.as_deref(),
            ProcessorType::Lowercase,
        ),
        Processor::Uppercase {
            field,
            target_field,
            ..
        } => apply_string_transform(
            document,
            field,
            target_field.as_deref(),
            ProcessorType::Uppercase,
        ),
        Processor::Trim {
            field,
            target_field,
            ..
        } => apply_string_transform(
            document,
            field,
            target_field.as_deref(),
            ProcessorType::Trim,
        ),
    }
}

// ---------- Pipeline execution ----------

pub fn execute_pipeline(document: &Value, processors: &[Processor]) -> ExecutionResult {
    let mut steps = Vec::new();
    let mut current = document.clone();

    for processor in processors {
        let before = current.clone();

        // Check if condition is present and evaluates to false!
        if let Some(cond) = processor.if_condition() {
            match evaluate_if_condition(&before, cond) {
                Ok(true) => {} // Proceed
                Ok(false) => {
                    // Skip processor!
                    steps.push(TraceStep {
                        processor_id: processor.id().to_string(),
                        processor_type: processor.processor_type(),
                        before: before.clone(),
                        after: before.clone(),
                        changed_paths: Vec::new(),
                        error: None,
                        ignored: true,
                    });
                    continue;
                }
                Err(e) => {
                    if processor.ignore_failure() {
                        steps.push(TraceStep {
                            processor_id: processor.id().to_string(),
                            processor_type: processor.processor_type(),
                            before: before.clone(),
                            after: before.clone(),
                            changed_paths: Vec::new(),
                            error: Some(format!("If-condition error: {}", e)),
                            ignored: true,
                        });
                        continue;
                    } else {
                        steps.push(TraceStep {
                            processor_id: processor.id().to_string(),
                            processor_type: processor.processor_type(),
                            before: before.clone(),
                            after: before.clone(),
                            changed_paths: Vec::new(),
                            error: Some(format!("If-condition error: {}", e)),
                            ignored: false,
                        });
                        return ExecutionResult {
                            steps,
                            final_document: before,
                            error: Some(format!(
                                "Processor \"{}\" failed on if-condition evaluation: {}",
                                processor.id(),
                                e
                            )),
                        };
                    }
                }
            }
        }

        // Apply processor
        match apply_processor(&before, processor) {
            Ok(after) => {
                let changed_paths = collect_changed_paths(&before, &after, "");
                steps.push(TraceStep {
                    processor_id: processor.id().to_string(),
                    processor_type: processor.processor_type(),
                    before,
                    after: after.clone(),
                    changed_paths,
                    error: None,
                    ignored: false,
                });
                current = after;
            }
            Err(message) => {
                if processor.ignore_failure() {
                    steps.push(TraceStep {
                        processor_id: processor.id().to_string(),
                        processor_type: processor.processor_type(),
                        before: before.clone(),
                        after: before.clone(),
                        changed_paths: Vec::new(),
                        error: Some(format!("Failed but ignored: {}", message)),
                        ignored: true,
                    });
                    current = before; // Chain continues with unchanged doc
                } else {
                    steps.push(TraceStep {
                        processor_id: processor.id().to_string(),
                        processor_type: processor.processor_type(),
                        before: before.clone(),
                        after: before.clone(),
                        changed_paths: Vec::new(),
                        error: Some(message.clone()),
                        ignored: false,
                    });
                    return ExecutionResult {
                        steps,
                        final_document: before,
                        error: Some(format!(
                            "Processor \"{}\" failed: {}",
                            processor.id(),
                            message
                        )),
                    };
                }
            }
        }
    }

    ExecutionResult {
        steps,
        final_document: current,
        error: None,
    }
}

// ---------- Factory ----------

pub fn default_processor(processor_type: ProcessorType) -> Processor {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let id = format!(
        "{}-{}-{}",
        processor_type.as_str(),
        ts,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    match processor_type {
        ProcessorType::Set => Processor::Set {
            id,
            field: "new_field".to_string(),
            value: Value::String("value".to_string()),
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Remove => Processor::Remove {
            id,
            fields: vec!["field_to_remove".to_string()],
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Json => Processor::Json {
            id,
            field: "payload".to_string(),
            target_field: Some("payload_object".to_string()),
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Reroute => Processor::Reroute {
            id,
            dataset: "generic".to_string(),
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Convert => Processor::Convert {
            id,
            field: "status_code".to_string(),
            target_field: Some("status_code".to_string()),
            convert_to: ConvertType::Integer,
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Lowercase => Processor::Lowercase {
            id,
            field: "level".to_string(),
            target_field: Some("level".to_string()),
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Uppercase => Processor::Uppercase {
            id,
            field: "service".to_string(),
            target_field: Some("service".to_string()),
            ignore_failure: false,
            if_condition: None,
        },
        ProcessorType::Trim => Processor::Trim {
            id,
            field: "message".to_string(),
            target_field: Some("message".to_string()),
            ignore_failure: false,
            if_condition: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_path_simple() {
        assert_eq!(split_path("a.b.c").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_path_brackets() {
        assert_eq!(split_path("a[0]").unwrap(), vec!["a", "0"]);
        assert_eq!(split_path("a[\"b\"]").unwrap(), vec!["a", "b"]);
        assert_eq!(split_path("a['b.c']").unwrap(), vec!["a", "b.c"]);
    }

    #[test]
    fn test_set_and_get() {
        let doc = serde_json::json!({"a": 1});
        let updated = set_by_path(&doc, "b.c", Value::String("hello".to_string())).unwrap();
        assert_eq!(
            get_by_path(&updated, "b.c"),
            Some(&Value::String("hello".to_string()))
        );
    }

    #[test]
    fn test_remove() {
        let doc = serde_json::json!({"a": {"b": 1, "c": 2}});
        let updated = remove_by_path(&doc, "a.b").unwrap();
        assert_eq!(get_by_path(&updated, "a.b"), None);
        assert_eq!(get_by_path(&updated, "a.c"), Some(&Value::Number(2.into())));
    }

    #[test]
    fn test_painless_evaluator() {
        let doc = serde_json::json!({
            "status": 200,
            "level": "error",
            "message": "failed to connect",
            "payload": {
                "subfield": "hello"
            }
        });
        assert!(evaluate_if_condition(&doc, "ctx.status == 200").unwrap());
        assert!(!evaluate_if_condition(&doc, "ctx.status == 500").unwrap());
        assert!(evaluate_if_condition(&doc, "ctx.level == 'error'").unwrap());
        assert!(evaluate_if_condition(&doc, "ctx.message.contains('fail')").unwrap());

        // Safe navigation tests
        assert!(evaluate_if_condition(&doc, "ctx?.status == 200").unwrap());
        assert!(evaluate_if_condition(&doc, "ctx?.payload?.subfield == 'hello'").unwrap());
        assert!(evaluate_if_condition(&doc, "ctx?.missing?.field == null").unwrap());
        assert!(evaluate_if_condition(&doc, "ctx.payload?.missing == null").unwrap());

        // Unsafe navigation fails on missing path
        assert!(evaluate_if_condition(&doc, "ctx.missing.field == null").is_err());
    }

    #[test]
    fn test_execute_pipeline() {
        let doc = serde_json::json!({
            "payload": "{\"message\":\" hello \",\"status\":\"200\"}"
        });
        let processors = vec![
            Processor::Json {
                id: "json-1".to_string(),
                field: "payload".to_string(),
                target_field: Some("payload".to_string()),
                ignore_failure: false,
                if_condition: None,
            },
            Processor::Convert {
                id: "convert-1".to_string(),
                field: "payload.status".to_string(),
                target_field: Some("payload.status".to_string()),
                convert_to: ConvertType::Integer,
                ignore_failure: false,
                if_condition: None,
            },
            Processor::Trim {
                id: "trim-1".to_string(),
                field: "payload.message".to_string(),
                target_field: Some("payload.message".to_string()),
                ignore_failure: false,
                if_condition: None,
            },
        ];
        let result = execute_pipeline(&doc, &processors);
        assert!(result.error.is_none());
        assert_eq!(result.steps.len(), 3);
        let status = get_by_path(&result.final_document, "payload.status");
        assert_eq!(status, Some(&Value::Number(200.into())));
        let message = get_by_path(&result.final_document, "payload.message");
        assert_eq!(message, Some(&Value::String("hello".to_string())));
    }
}
