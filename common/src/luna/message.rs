use serde::de::DeserializeOwned;
use crate::luna::Message;

impl Message {
    pub fn deserialize<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        return serde_json::from_value(self.value);
    }
}
