use std::fmt::Write;

use ares_device_lib::Device;
use serde_json::Value;

/// Prints the device list as a padded table, mirroring the reference CLI:
/// name (with a `(default)` marker), deviceinfo, connection, profile, passphrase.
pub(crate) fn print_list(devices: &[Device]) {
    let headers = ["name", "deviceinfo", "connection", "profile", "passphrase"];
    let rows: Vec<[String; 5]> = devices.iter().map(device_row).collect();
    print_table(&headers, &rows);
}

/// Prints every field of every device, like the reference CLI's `--listfull`.
pub(crate) fn print_list_full(devices: &[Device]) {
    for device in devices {
        println!("name : {}", device.name);
        if let Ok(Value::Object(mut obj)) = serde_json::to_value(device) {
            obj.remove("name");
            print!("{}", convert_json_to_list(&Value::Object(obj), 0));
        }
        println!();
    }
}

fn device_row(device: &Device) -> [String; 5] {
    let name = if device.default == Some(true) {
        format!("{} (default)", device.name)
    } else {
        device.name.clone()
    };
    [
        name,
        format!("{}@{}:{}", device.username, device.host, device.port),
        String::from("ssh"),
        device.profile.clone(),
        device.passphrase.clone().unwrap_or_default(),
    ]
}

fn print_table<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let mut widths = headers.map(str::len);
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let print_row = |cells: &[String; N]| {
        let line: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| format!("{cell:<width$}", width = widths[i]))
            .collect();
        println!("{}", line.join("  ").trim_end());
    };

    let header_row: [String; N] = std::array::from_fn(|i| headers[i].to_string());
    let separator_row: [String; N] = std::array::from_fn(|i| "-".repeat(widths[i]));
    print_row(&header_row);
    print_row(&separator_row);
    for row in rows {
        print_row(row);
    }
}

/// Renders a JSON value into an indented text list (one extra `-` per level),
/// matching the reference CLI's `convertJsonToList`.
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
                if val.is_object() || val.is_array() || val.is_null() {
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

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
