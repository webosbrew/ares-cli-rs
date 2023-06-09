use std::process::exit;

use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;

use crate::close::CloseApp;
use crate::launch::LaunchApp;
use crate::running::ListRunning;

mod close;
mod launch;
mod running;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        help = "Specify DEVICE to use"
    )]
    device: Option<String>,
    #[arg(short, long, group = "action", help = "Close a running app")]
    close: bool,
    #[arg(short, long, group = "action", help = "List running apps")]
    running: bool,
    #[arg(
        short,
        long,
        value_name = "PARAMS",
        help = "Launch/Close an app with the specified parameters"
    )]
    params: Vec<String>,
    #[arg(value_name = "APP_ID", help = "An app id described in appinfo.json")]
    app_id: Option<String>,
}

#[derive(Serialize, Debug)]
struct LaunchParams {
    id: String,
    subscribe: bool,
    #[serde(skip_serializing_if = "Value::is_null")]
    params: Value,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LaunchResponse {
    return_value: bool,
    error_code: Option<i32>,
    error_text: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let device = manager.find_or_default(cli.device).unwrap();
    if device.is_none() {
        eprintln!("Device not found");
        exit(1);
    }
    let device = device.unwrap();
    let session = device.new_session().unwrap();

    if cli.running {
        session.list_running();
        return;
    }
    if cli.app_id.is_none() {
        Cli::parse_from(vec!["", "--help"]);
        return;
    }

    let mut params: Value = Value::Null;
    if !cli.params.is_empty() {
        let mut map = Map::new();
        for p in cli.params {
            if p.starts_with("{") {
                match serde_json::from_str::<Value>(&p) {
                    Ok(mut value) => map.append(&mut value.as_object_mut().unwrap()),
                    Err(e) => eprintln!("Ignoring param `{p}` as error occurred parsing it: {e:?}"),
                }
            } else {
                let mut split = p.splitn(2, '=');
                if let (Some(left), Some(right)) = (split.next(), split.next()) {
                    map.insert(String::from(left), json!(right));
                } else {
                    eprintln!("Ignoring unrecognized param `{p}`")
                }
            }
        }
        params = Value::Object(map);
    }
    if cli.close {
        session.close_app(cli.app_id.unwrap(), params);
    } else {
        session.launch_app(cli.app_id.unwrap(), params);
    }
}
