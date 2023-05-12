use libssh_rs::Session;
use serde_json::Value;
use std::process::exit;

use ares_connection_lib::luna::Luna;

use crate::{LaunchParams, LaunchResponse};

pub(crate) trait CloseApp {
    fn close_app(&self, app_id: String, params: Value);
}
impl CloseApp for Session {
    fn close_app(&self, app_id: String, params: Value) {
        let response: LaunchResponse = self
            .call(
                "luna://com.webos.applicationManager/dev/closeByAppId",
                &LaunchParams {
                    id: app_id.clone(),
                    subscribe: false,
                    params,
                },
                true,
            )
            .unwrap();
        if response.return_value {
            println!("Closed application {app_id}");
        } else {
            eprintln!(
                "Failed to close {app_id}: {} ({})",
                response.error_text.unwrap_or(String::from("unknown error")),
                response.error_code.unwrap_or(-1)
            );
            exit(1);
        }
    }
}
