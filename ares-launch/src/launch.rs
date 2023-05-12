use libssh_rs::Session;
use serde_json::Value;
use std::process::exit;

use ares_connection_lib::luna::Luna;

use crate::{LaunchParams, LaunchResponse};

pub(crate) trait LaunchApp {
    fn launch_app(&self, app_id: String, params: Value);
}

impl LaunchApp for Session {
    fn launch_app(&self, app_id: String, params: Value) {
        let response: LaunchResponse = self
            .call(
                "luna://com.webos.applicationManager/launch",
                &LaunchParams {
                    id: app_id.clone(),
                    subscribe: false,
                    params,
                },
                true,
            )
            .unwrap();
        if response.return_value {
            println!("Launched application {app_id}");
        } else {
            eprintln!(
                "Failed to launch {app_id}: {} ({})",
                response.error_text.unwrap_or(String::from("unknown error")),
                response.error_code.unwrap_or(-1)
            );
            exit(1);
        }
    }
}
