use std::io::{Error, ErrorKind, Read, Result};

use serde::Deserialize;

use crate::ParseFrom;

#[derive(Debug, Deserialize)]
pub struct ServiceInfo {
    pub id: String,
    pub description: Option<String>,
    pub engine: Option<String>,
    pub executable: Option<String>,
}

impl ParseFrom for ServiceInfo {
    fn parse_from<R: Read>(reader: R) -> Result<ServiceInfo> {
        serde_json::from_reader(reader).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid services.json: {e:?}"),
            )
        })
    }
}
