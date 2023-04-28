use libssh_rs::Session;
use serde::Deserialize;

use common::luna::{Luna, LunaEmptyPayload};

pub(crate) trait ListApps {
    fn list_apps(&self);
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListAppsResponse {
    pub apps: Vec<App>,
    pub return_value: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    pub version: String,
    pub r#type: String,
    pub title: String,
    pub vendor: Option<String>,
}

impl ListApps for Session {
    fn list_apps(&self) {
        let resp: ListAppsResponse = self
            .call(
                "luna://com.webos.applicationManager/dev/listApps",
                LunaEmptyPayload::default(),
                true,
            )
            .unwrap();
        for app in resp.apps {
            println!("{}", app.id);
        }
    }
}
