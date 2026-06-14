use std::net::Ipv4Addr;

use ares_device_lib::Device;
use serde_json::{Map, Value, json};

/// Builds a brand-new device from the `--add` name and parsed `--info` fields,
/// filling in sensible defaults for anything the user did not provide.
pub(crate) fn build_device(name: &str, info: &Map<String, Value>) -> Result<Device, String> {
    let mut map = Map::new();
    map.insert(String::from("profile"), json!("ose"));
    map.insert(String::from("port"), json!(9922));
    map.insert(String::from("username"), json!("root"));
    map.insert(String::from("files"), json!("sftp"));
    for (key, value) in info {
        map.insert(key.clone(), value.clone());
    }
    map.insert(String::from("name"), json!(name));

    if !map.contains_key("host") {
        return Err(String::from("Missing required field: host"));
    }
    apply_auth(&mut map, info);
    validate(&map)?;
    deserialize(map)
}

/// Applies parsed `--info` changes on top of an existing device (for `--modify`).
pub(crate) fn modified_device(
    existing: &Device,
    info: &Map<String, Value>,
) -> Result<Device, String> {
    let Ok(Value::Object(mut map)) = serde_json::to_value(existing) else {
        return Err(String::from("Failed to read existing device"));
    };
    for (key, value) in info {
        map.insert(key.clone(), value.clone());
    }
    apply_auth(&mut map, info);
    validate(&map)?;
    deserialize(map)
}

/// Parses `--info` arguments, accepting either a single JSON object string or
/// repeated `key=value` pairs, and normalizes them to device field names.
pub(crate) fn parse_info(info: &[String]) -> Result<Map<String, Value>, String> {
    if let [single] = info
        && single.trim_start().starts_with('{')
    {
        // Allow single-quoted JSON for shell friendliness.
        let value: Value = serde_json::from_str(&single.replace('\'', "\""))
            .map_err(|e| format!("Invalid JSON in --info: {e}"))?;
        let Value::Object(obj) = value else {
            return Err(String::from("--info JSON must be an object"));
        };
        let mut map = Map::new();
        for (key, value) in obj {
            insert_field(&mut map, &key, &value)?;
        }
        return Ok(map);
    }

    let mut map = Map::new();
    for item in info {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| format!("Invalid --info '{item}', expected key=value"))?;
        insert_field(
            &mut map,
            key.trim(),
            &Value::String(value.trim().to_string()),
        )?;
    }
    Ok(map)
}

/// Maps an input key (and its raw value) onto the device's JSON field shape.
fn insert_field(map: &mut Map<String, Value>, key: &str, value: &Value) -> Result<(), String> {
    let text = || value.as_str().map(str::to_string).unwrap_or_default();
    match key {
        "host" | "ipAddress" => {
            map.insert(String::from("host"), json!(text()));
        }
        "port" => {
            let port = match value {
                Value::Number(n) => n.as_u64(),
                Value::String(s) => s.parse::<u64>().ok(),
                _ => None,
            }
            .ok_or_else(|| String::from("port must be a number"))?;
            map.insert(String::from("port"), json!(port));
        }
        "username" | "user" => {
            map.insert(String::from("username"), json!(text()));
        }
        "profile" => {
            map.insert(String::from("profile"), json!(text()));
        }
        "description" => {
            map.insert(String::from("description"), json!(text()));
        }
        "password" => {
            map.insert(String::from("password"), json!(text()));
        }
        "passphrase" => {
            map.insert(String::from("passphrase"), json!(text()));
        }
        "privateKey" | "openSsh" => {
            map.insert(String::from("privateKey"), json!({ "openSsh": text() }));
        }
        "openSshPath" | "keyPath" => {
            map.insert(String::from("privateKey"), json!({ "openSshPath": text() }));
        }
        "files" => {
            map.insert(String::from("files"), json!(text()));
        }
        "default" => {
            let flag = matches!(value, Value::Bool(true))
                || value
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("true"));
            map.insert(String::from("default"), json!(flag));
        }
        other => return Err(format!("Unknown --info field: {other}")),
    }
    Ok(())
}

/// Ensures only one authentication method is stored, based on what the user
/// explicitly provided: a password clears any key, and a key clears a password.
fn apply_auth(map: &mut Map<String, Value>, info: &Map<String, Value>) {
    let has_key = info.contains_key("privateKey");
    let has_password = info.contains_key("password");
    if has_key {
        map.remove("password");
    } else if has_password {
        map.remove("privateKey");
        map.remove("passphrase");
    }
}

fn validate(map: &Map<String, Value>) -> Result<(), String> {
    let name = map
        .get("name")
        .and_then(Value::as_str)
        .ok_or("Device name is required")?;
    validate_name(name)?;

    let host = map
        .get("host")
        .and_then(Value::as_str)
        .ok_or("Device host is required")?;
    validate_host(host)?;

    validate_port(map.get("port"))?;
    Ok(())
}

fn validate_name(name: &str) -> Result<(), String> {
    match name.chars().next() {
        None => Err(String::from("Device name must not be empty")),
        Some('$' | '%') => Err(String::from("Device name must not start with '$' or '%'")),
        Some(_) => Ok(()),
    }
}

fn validate_host(host: &str) -> Result<(), String> {
    if host == "localhost" || host.parse::<Ipv4Addr>().is_ok() {
        Ok(())
    } else {
        Err(format!(
            "Invalid host '{host}': expected 'localhost' or an IPv4 address"
        ))
    }
}

fn validate_port(port: Option<&Value>) -> Result<(), String> {
    let number = match port {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.parse::<u64>().ok(),
        _ => None,
    }
    .ok_or("Device port is required")?;
    if (1..=65535).contains(&number) {
        Ok(())
    } else {
        Err(format!(
            "Invalid port {number}: must be between 1 and 65535"
        ))
    }
}

fn deserialize(map: Map<String, Value>) -> Result<Device, String> {
    serde_json::from_value(Value::Object(map)).map_err(|e| format!("Invalid device info: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{build_device, parse_info};

    fn info(args: &[&str]) -> serde_json::Map<String, serde_json::Value> {
        parse_info(&args.iter().map(ToString::to_string).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn parses_key_value_with_aliases() {
        let map = info(&["ipAddress=1.2.3.4", "user=root", "port=9922"]);
        assert_eq!(map["host"], "1.2.3.4");
        assert_eq!(map["username"], "root");
        assert_eq!(map["port"], 9922);
    }

    #[test]
    fn parses_json_object() {
        let map = info(&["{'host':'1.2.3.4','port':9922}"]);
        assert_eq!(map["host"], "1.2.3.4");
        assert_eq!(map["port"], 9922);
    }

    #[test]
    fn add_fills_defaults_and_requires_host() {
        let device = build_device("tv", &info(&["host=1.2.3.4"])).unwrap();
        assert_eq!(device.username, "root");
        assert_eq!(device.port, 9922);
        assert_eq!(device.profile, "ose");

        assert!(build_device("tv", &info(&["username=root"])).is_err());
    }

    #[test]
    fn rejects_invalid_host_and_port() {
        assert!(build_device("tv", &info(&["host=999.1.1.1"])).is_err());
        assert!(build_device("tv", &info(&["host=1.2.3.4", "port=70000"])).is_err());
    }

    #[test]
    fn modifying_to_password_clears_existing_key() {
        let with_key = build_device("tv", &info(&["host=1.2.3.4", "openSsh=k"])).unwrap();
        assert!(with_key.private_key.is_some());

        let with_password = super::modified_device(&with_key, &info(&["password=p"])).unwrap();
        assert!(with_password.private_key.is_none());
        assert_eq!(with_password.password.as_deref(), Some("p"));
    }
}
