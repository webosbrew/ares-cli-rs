use std::fmt::Write;

use ares_connection_lib::luna::{Luna, LunaEmptyPayload};
use libssh_rs::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub(crate) trait ListApps {
    fn list_apps(&self, list_full: bool, type_filter: Option<&str>);
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListAppsResponse {
    pub apps: Vec<App>,
    pub return_value: bool,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    pub version: String,
    pub r#type: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    // Keep every remaining field so `--listfull` can render the full app info.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ListApps for Session {
    fn list_apps(&self, list_full: bool, type_filter: Option<&str>) {
        let resp: ListAppsResponse = self
            .call(
                "luna://com.webos.applicationManager/dev/listApps",
                LunaEmptyPayload::default(),
                true,
            )
            .unwrap();
        for app in resp.apps {
            // Mirror the reference CLI: hide non-visible apps (non-signage profile).
            if !app.visible.unwrap_or(false) {
                continue;
            }
            if type_filter.is_some_and(|t| app.r#type != t) {
                continue;
            }
            if list_full {
                println!("id : {}", app.id);
                if let Ok(Value::Object(mut obj)) = serde_json::to_value(&app) {
                    obj.remove("id");
                    print!("{}", convert_json_to_list(&Value::Object(obj), 0));
                }
                println!();
            } else {
                println!("{}", app.id);
            }
        }
    }
}

/// Renders a JSON value into an indented text list, matching the reference CLI's
/// `convertJsonToList`. Each nesting level is prefixed with one additional `-`.
fn convert_json_to_list(value: &Value, level: usize) -> String {
    let prefix = "-".repeat(level);
    let mut out = String::new();
    match value {
        Value::String(s) => {
            let _ = writeln!(out, "{prefix}{s}");
        }
        Value::Array(arr) if !arr.is_empty() => {
            for item in arr {
                out.push_str(&convert_json_to_list(item, level));
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                if is_nested(val) {
                    let _ = writeln!(out, "{prefix}{key}");
                    out.push_str(&convert_json_to_list(val, level + 1));
                } else {
                    let _ = writeln!(out, "{prefix}{key} : {}", scalar_to_string(val));
                }
            }
        }
        _ => {}
    }
    out
}

/// In JavaScript `typeof` reports objects, arrays and `null` all as `"object"`,
/// so the reference implementation recurses into each of these.
fn is_nested(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_) | Value::Null)
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
