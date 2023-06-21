use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct GetInfoReponse {
    pub head_block_time: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
