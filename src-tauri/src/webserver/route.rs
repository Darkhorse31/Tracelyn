use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    pub pattern: String,
    pub replacement: String,
}

static NUMERIC_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/[0-9]+").expect("regex"));
static UUID_SEGMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
    )
    .expect("regex")
});
static HASH_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/[0-9a-fA-F]{32,64}").expect("regex"));

pub fn normalize_path(path: &str, manual_rules: &[RouteRule]) -> String {
    let mut p = path.to_string();
    for rule in manual_rules {
        if let Ok(re) = Regex::new(&rule.pattern) {
            p = re.replace_all(&p, rule.replacement.as_str()).to_string();
        }
    }
    p = UUID_SEGMENT.replace_all(&p, "/:id").to_string();
    p = NUMERIC_SEGMENT.replace_all(&p, "/:id").to_string();
    p = HASH_SEGMENT.replace_all(&p, "/:id").to_string();
    p
}

#[derive(Debug, Clone, Default)]
pub struct RouteNormalizer {
    rules: Vec<RouteRule>,
}

impl RouteNormalizer {
    pub fn new(rules: Vec<RouteRule>) -> Self {
        Self { rules }
    }

    pub fn add_rule(&mut self, rule: RouteRule) {
        self.rules.push(rule);
    }

    pub fn normalize(&self, path: &str) -> String {
        normalize_path(path, &self.rules)
    }
}
