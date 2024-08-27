#![allow(clippy::needless_return)]

use std::io::{Error, ErrorKind, Read, Result};

use serde::Deserialize;

use crate::ParseFrom;

#[derive(Debug, Deserialize)]
pub struct AppInfo {
    pub id: String,
    pub version: String,
    pub r#type: String,
    pub main: String,
    pub title: String,
    pub vendor: Option<String>,
}

impl ParseFrom for AppInfo {
    fn parse_from<R: Read>(reader: R) -> Result<AppInfo> {
        serde_json::from_reader(reader).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid appinfo.json: {e:?}"),
            )
        })
    }
}
