use egui::Ui;

use crate::core::config::SavedCommand;

#[derive(Debug, Clone, Default)]
pub struct ConsoleState {
    pub selected_cluster: String,
    pub target: ConsoleTarget,
    pub method: String,
    pub path: String,
    pub body: String,
    pub response: String,
    pub jq_filter: String,
    pub jq_error: Option<String>,
    pub is_loading: bool,
}

impl ConsoleState {
    pub fn new() -> Self {
        Self {
            method: "GET".to_string(),
            path: "/_cluster/health".to_string(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleTarget {
    #[default]
    Elasticsearch,
    Kibana,
}

impl ConsoleTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConsoleTarget::Elasticsearch => "ES",
            ConsoleTarget::Kibana => "Kibana",
        }
    }
}

pub fn render_console_module(
    ui: &mut Ui,
    state: &mut ConsoleState,
    clusters: &[(String, Option<String>)], // (name, kibana_host)
    saved_commands: &[SavedCommand],
    on_send: &mut Option<(String, ConsoleTarget, String, String, Option<String>)>,
    on_save: &mut Option<(String, String, String, String)>,
) {
    ui.heading("Elastic Console");
    ui.add_space(16.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    egui::Frame::new()
        .fill(crate::ui::theme::Theme::BG_CARD)
        .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
        .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
        .show(ui, |ui| {
            // Cluster + Target selection
            ui.horizontal(|ui| {
                ui.label("Cluster:");
                egui::ComboBox::from_id_salt("console_cluster")
                    .selected_text(&state.selected_cluster)
                    .show_ui(ui, |ui| {
                        for (name, _) in clusters {
                            ui.selectable_value(
                                &mut state.selected_cluster,
                                name.clone(),
                                name,
                            );
                        }
                    });

                ui.label("Target:");
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut state.target,
                        ConsoleTarget::Elasticsearch,
                        ConsoleTarget::Elasticsearch.as_str(),
                    );
                    ui.selectable_value(
                        &mut state.target,
                        ConsoleTarget::Kibana,
                        ConsoleTarget::Kibana.as_str(),
                    );
                });
            });

            // Target warning if Kibana selected but no host configured
            if state.target == ConsoleTarget::Kibana {
                let has_kibana = clusters
                    .iter()
                    .find(|(n, _)| n == &state.selected_cluster)
                    .and_then(|(_, k)| k.as_ref())
                    .is_some();
                if !has_kibana {
                    ui.colored_label(
                        crate::ui::theme::Theme::WARNING,
                        "No Kibana host configured for this cluster.",
                    );
                }
            }

            // Saved commands dropdown
            if !saved_commands.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Saved:");
                    egui::ComboBox::from_id_salt("console_saved")
                        .selected_text("Select command...")
                        .show_ui(ui, |ui| {
                            for cmd in saved_commands {
                                if ui
                                    .selectable_label(false, format!(
                                        "[{}] {}",
                                        cmd.target.to_uppercase(),
                                        cmd.name
                                    ))
                                    .clicked()
                                {
                                    state.method = cmd.method.clone();
                                    state.path = cmd.path.clone();
                                    state.body = cmd.body.clone();
                                    state.target = if cmd.target == "kibana" {
                                        ConsoleTarget::Kibana
                                    } else {
                                        ConsoleTarget::Elasticsearch
                                    };
                                }
                            }
                        });
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Method:");
                egui::ComboBox::from_id_salt("console_method")
                    .selected_text(&state.method)
                    .show_ui(ui, |ui| {
                        for m in ["GET", "POST", "PUT", "DELETE", "HEAD"] {
                            ui.selectable_value(&mut state.method, m.to_string(), m);
                        }
                    });

                ui.label("Path:");
                ui.text_edit_singleline(&mut state.path);

                let button = ui.button("Send");
                if state.is_loading {
                    ui.spinner();
                }
                if button.clicked() && !state.is_loading {
                    state.is_loading = true;
                    state.jq_error = None;
                    let body = if state.body.trim().is_empty() {
                        None
                    } else {
                        Some(state.body.clone())
                    };
                    *on_send = Some((
                        state.selected_cluster.clone(),
                        state.target,
                        state.method.clone(),
                        state.path.clone(),
                        body,
                    ));
                }
            });

            // Save command button
            ui.horizontal(|ui| {
                if ui.small_button("💾 Save Command").clicked() {
                    *on_save = Some((
                        state.method.clone(),
                        state.path.clone(),
                        state.body.clone(),
                        if state.target == ConsoleTarget::Kibana {
                            "kibana".to_string()
                        } else {
                            "es".to_string()
                        },
                    ));
                }
            });

            ui.add_space(8.0);
            ui.label("Body:");
            ui.add_sized(
                [ui.available_width(), 100.0],
                egui::TextEdit::multiline(&mut state.body).code_editor(),
            );

            // jq filter
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("jq filter:");
                ui.text_edit_singleline(&mut state.jq_filter);
                if ui.small_button("Apply").clicked() {
                    state.jq_error = None;
                }
                if !state.jq_filter.is_empty() {
                    if ui.small_button("Clear").clicked() {
                        state.jq_filter.clear();
                        state.jq_error = None;
                    }
                }
            });
            if let Some(ref err) = state.jq_error {
                ui.colored_label(crate::ui::theme::Theme::DANGER, err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Response:");
                if ui.small_button("📋 Copy").clicked() {
                    ui.ctx().copy_text(state.response.clone());
                }
                if ui.small_button("Clear").clicked() {
                    state.response.clear();
                    state.jq_error = None;
                }
            });
            ui.add_sized(
                [ui.available_width(), 200.0],
                egui::TextEdit::multiline(&mut state.response).code_editor(),
            );
        });
}

/// Apply a lightweight jq-style filter to a JSON string.
/// Supports:
///   `.`               identity
///   `.key`            object field access
///   `.key.subkey`     chained access
///   `.key[0]`         array index access
///   `.key[]`          array iteration (unwraps array into multiple values)
///   `.key | .subkey`  pipe syntax
pub fn apply_jq_filter(input: &str, filter: &str) -> Result<String, String> {
    if filter.trim().is_empty() || filter.trim() == "." {
        return Ok(input.to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("Invalid JSON: {}", e))?;

    let outputs = run_filter(&value, filter.trim())?;

    if outputs.is_empty() {
        Ok("null".to_string())
    } else if outputs.len() == 1 {
        serde_json::to_string_pretty(&outputs[0])
            .map_err(|e| format!("JSON formatting error: {}", e))
    } else {
        let items: Result<Vec<String>, String> = outputs
            .iter()
            .map(|v| {
                serde_json::to_string_pretty(v)
                    .map_err(|e| format!("JSON formatting error: {}", e))
            })
            .collect();
        Ok(format!("[\n{}\n]", items?.join(",\n")))
    }
}

fn run_filter(value: &serde_json::Value, filter: &str) -> Result<Vec<serde_json::Value>, String> {
    let parts: Vec<&str> = filter.split('|').map(|s| s.trim()).collect();
    let mut current = vec![value.clone()];

    for part in parts {
        let mut next = Vec::new();
        for val in &current {
            let mut cursor = val;
            let mut tokens = tokenize(part)?;
            // Handle leading `.`
            if tokens.first() == Some(&Token::Dot) {
                tokens.remove(0);
            }

            for token in &tokens {
                match token {
                    Token::Key(key) => {
                        if let Some(obj) = cursor.as_object() {
                            match obj.get(key) {
                                Some(v) => cursor = v,
                                None => {
                                    return Err(format!(
                                        "Key '{}' not found",
                                        key
                                    ))
                                }
                            }
                        } else {
                            return Err(format!(
                                "Cannot index '{}' into non-object",
                                key
                            ));
                        }
                    }
                    Token::Index(idx) => {
                        if let Some(arr) = cursor.as_array() {
                            let i = if *idx < 0 {
                                arr.len().saturating_sub(idx.unsigned_abs() as usize)
                            } else {
                                *idx as usize
                            };
                            match arr.get(i) {
                                Some(v) => cursor = v,
                                None => {
                                    return Err(format!(
                                        "Index {} out of bounds (len {})",
                                        idx,
                                        arr.len()
                                    ))
                                }
                            }
                        } else {
                            return Err(format!(
                                "Cannot index [{}] into non-array",
                                idx
                            ));
                        }
                    }
                    Token::Iterator => {
                        if let Some(arr) = cursor.as_array() {
                            for item in arr {
                                next.push(item.clone());
                            }
                            // Array iteration produces multiple values; move to next input
                            cursor = &serde_json::Value::Null;
                            break;
                        } else {
                            return Err(format!(
                                "Cannot iterate over non-array"
                            ));
                        }
                    }
                    Token::Dot => {}
                }
            }
            // Only push if we didn't already push via Iterator
            if next.is_empty() || cursor != &serde_json::Value::Null {
                next.push(cursor.clone());
            }
        }
        current = next;
    }

    Ok(current)
}

#[derive(Debug, PartialEq)]
enum Token {
    Dot,
    Key(String),
    Index(i64),
    Iterator,
}

fn tokenize(filter: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = filter.chars().peekable();

    // Optional leading dot
    if chars.peek() == Some(&'.') {
        chars.next();
        tokens.push(Token::Dot);
    }

    while chars.peek().is_some() {
        match chars.peek() {
            Some('.') => {
                chars.next();
                tokens.push(Token::Dot);
            }
            Some('[') => {
                chars.next(); // consume '['
                if chars.peek() == Some(&']') {
                    chars.next();
                    tokens.push(Token::Iterator);
                } else {
                    let mut num_str = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == ']' {
                            chars.next();
                            break;
                        }
                        num_str.push(c);
                        chars.next();
                    }
                    let idx: i64 = num_str
                        .trim()
                        .parse()
                        .map_err(|_| format!("Invalid array index: '{}'", num_str))?;
                    tokens.push(Token::Index(idx));
                }
            }
            Some(&c) if c.is_alphabetic() || c == '_' || c == '@' => {
                let mut key = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' || c == '@' || c == '-' || c == '.' {
                        // Stop at dot-separator (but allow dots inside quoted/escaped keys? keep simple)
                        if c == '.' {
                            break;
                        }
                        key.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Key(key));
            }
            Some(&c) if c.is_whitespace() => {
                chars.next();
            }
            Some(&c) => {
                return Err(format!("Unexpected character '{}' in filter", c));
            }
            None => break,
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        let json = r#"{"a": 1}"#;
        assert_eq!(apply_jq_filter(json, ".").unwrap(), json);
    }

    #[test]
    fn test_key_access() {
        let json = r#"{"name": "test", "value": 42}"#;
        let result = apply_jq_filter(json, ".name").unwrap();
        assert_eq!(result.trim(), "\"test\"");
    }

    #[test]
    fn test_nested_access() {
        let json = r#"{"outer": {"inner": 123}}"#;
        let result = apply_jq_filter(json, ".outer.inner").unwrap();
        assert_eq!(result.trim(), "123");
    }

    #[test]
    fn test_array_index() {
        let json = r#"{"items": [10, 20, 30]}"#;
        let result = apply_jq_filter(json, ".items[1]").unwrap();
        assert_eq!(result.trim(), "20");
    }

    #[test]
    fn test_array_iteration() {
        let json = r#"{"items": [1, 2, 3]}"#;
        let result = apply_jq_filter(json, ".items[]").unwrap();
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_pipe() {
        let json = r#"{"a": {"b": {"c": "hello"}}}"#;
        let result = apply_jq_filter(json, ".a | .b.c").unwrap();
        assert_eq!(result.trim(), "\"hello\"");
    }
}
