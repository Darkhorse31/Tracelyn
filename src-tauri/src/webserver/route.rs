use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Rejects rules whose pattern is empty or not a valid regex.
pub fn validate_rules(rules: &[RouteRule]) -> Result<(), String> {
    for rule in rules {
        if rule.pattern.trim().is_empty() {
            return Err("route rule pattern cannot be empty".into());
        }
        Regex::new(&rule.pattern)
            .map_err(|e| format!("invalid pattern `{}`: {e}", rule.pattern))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct RouteNormalizer {
    rules: Vec<RouteRule>,
}

impl RouteNormalizer {
    pub fn new(rules: Vec<RouteRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[RouteRule] {
        &self.rules
    }

    pub fn set_rules(&mut self, rules: Vec<RouteRule>) {
        self.rules = rules;
    }

    pub fn normalize(&self, path: &str) -> String {
        normalize_path(path, &self.rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, replacement: &str) -> RouteRule {
        RouteRule {
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn validate_rejects_empty_and_invalid_patterns() {
        assert!(validate_rules(&[rule(r"^/api/v\d+", "/api/v:ver")]).is_ok());
        assert!(validate_rules(&[rule("  ", "/x")]).is_err());
        assert!(validate_rules(&[rule("(unclosed", "/x")]).is_err());
    }

    #[test]
    fn set_rules_replaces_live_rules() {
        let mut normalizer = RouteNormalizer::default();
        assert_eq!(normalizer.normalize("/api/v2/users"), "/api/v2/users");
        normalizer.set_rules(vec![rule(r"^/api/v\d+", "/api/v:ver")]);
        assert_eq!(normalizer.normalize("/api/v2/users/42"), "/api/v:ver/users/:id");
        normalizer.set_rules(Vec::new());
        assert!(normalizer.rules().is_empty());
    }
}
