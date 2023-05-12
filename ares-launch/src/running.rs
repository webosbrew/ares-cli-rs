use std::process::exit;

use libssh_rs::Session;
use serde::Deserialize;
use serde_json::json;

use ares_connection_lib::luna::Luna;

pub(crate) trait ListRunning {
    fn list_running(&self);
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListRunningResponse {
    return_value: bool,
    error_code: Option<i32>,
    error_text: Option<String>,
    running: Option<Vec<RunningProcess>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RunningProcess {
    id: String,
}

impl ListRunning for Session {
    fn list_running(&self) {
        let response: ListRunningResponse = self
            .call(
                "luna://com.webos.applicationManager/dev/running",
                json!({"subscribe":false}),
                true,
            )
            .unwrap();
        if response.return_value {
            if let Some(running) = response.running {
                for proc in running {
                    println!("{}", proc.id);
                }
            }
        } else {
            eprintln!(
                "Failed to list running apps: {} ({})",
                response.error_text.unwrap_or(String::from("unknown error")),
                response.error_code.unwrap_or(-1)
            );
            exit(1);
        }
    }
}
