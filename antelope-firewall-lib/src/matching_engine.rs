use std::{collections::HashSet, sync::Arc};

use reqwest::Url;
use serde_json::Value;

use crate::{MatchingFn, RequestInfo};

pub struct MatchingEngine {
    rules: Vec<Box<MatchingFn>>,
}

impl MatchingEngine {
    pub fn new() -> Self {
        MatchingEngine { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: Box<MatchingFn>) {
        self.rules.push(rule);
    }

    pub async fn find_matching_urls(
        &self,
        request_info: Arc<RequestInfo>,
        body: Arc<Value>,
    ) -> HashSet<(Url, u64)> {
        let mut urls = HashSet::new();
        for rule in &self.rules {
            urls = rule((
                Arc::clone(&request_info),
                Arc::clone(&body),
                Arc::new(Value::Null),
                urls,
            ))
            .await;
        }
        urls
    }
}
