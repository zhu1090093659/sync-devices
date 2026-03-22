use regex::Regex;
use std::sync::LazyLock;

/// Sensitive patterns to detect and redact
struct SensitivePattern {
    regex: Regex,
    label: &'static str,
}

static PATTERNS: LazyLock<Vec<SensitivePattern>> = LazyLock::new(|| {
    vec![
        SensitivePattern {
            regex: Regex::new(r#"sk-[a-zA-Z0-9_\-]{20,}"#).unwrap(),
            label: "api_key",
        },
        SensitivePattern {
            regex: Regex::new(r#"ace_[a-zA-Z0-9]{20,}"#).unwrap(),
            label: "ace_token",
        },
        SensitivePattern {
            regex: Regex::new(r#"ghp_[a-zA-Z0-9]{36}"#).unwrap(),
            label: "github_pat",
        },
        SensitivePattern {
            regex: Regex::new(r#"gho_[a-zA-Z0-9]{36}"#).unwrap(),
            label: "github_oauth",
        },
        SensitivePattern {
            regex: Regex::new(r#"Bearer\s+[a-zA-Z0-9_\-\.]{20,}"#).unwrap(),
            label: "bearer_token",
        },
        SensitivePattern {
            regex: Regex::new(r#"[a-zA-Z0-9+/]{40,}={0,2}"#).unwrap(),
            label: "base64_secret",
        },
    ]
});

/// Result of scanning content for sensitive data
#[derive(Debug)]
pub struct ScanResult {
    /// Whether sensitive data was found
    pub has_sensitive: bool,
    /// List of detected sensitive patterns
    pub findings: Vec<Finding>,
}

#[derive(Debug)]
pub struct Finding {
    pub label: String,
    pub line: usize,
}

/// Scan content for sensitive patterns without modifying it
pub fn scan(content: &str) -> ScanResult {
    let mut findings = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        for pattern in PATTERNS.iter() {
            if pattern.regex.is_match(line) {
                findings.push(Finding {
                    label: pattern.label.to_string(),
                    line: line_num + 1,
                });
            }
        }
    }

    ScanResult {
        has_sensitive: !findings.is_empty(),
        findings,
    }
}

/// Redact sensitive patterns in content, replacing them with placeholders
pub fn redact(content: &str) -> String {
    let mut result = content.to_string();
    for pattern in PATTERNS.iter() {
        result = pattern
            .regex
            .replace_all(&result, format!("<REDACTED:{}>", pattern.label))
            .to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_api_key() {
        let content = r#"{"key": "sk-abc123def456ghi789jkl012mno345pqr678"}"#;
        let result = scan(content);
        assert!(result.has_sensitive);
        assert_eq!(result.findings[0].label, "api_key");
    }

    #[test]
    fn test_detects_github_pat() {
        let content = "token = \"ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\"";
        let result = scan(content);
        assert!(result.has_sensitive);
    }

    #[test]
    fn test_redacts_api_key() {
        let content = r#"ANTHROPIC_AUTH_TOKEN = "sk-abc123def456ghi789jkl012"#;
        let redacted = redact(content);
        assert!(redacted.contains("<REDACTED:api_key>"));
        assert!(!redacted.contains("sk-abc123"));
    }

    #[test]
    fn test_clean_content_passes() {
        let content = "model = \"claude-opus-4-6\"\nlanguage = \"zh\"";
        let result = scan(content);
        assert!(!result.has_sensitive);
    }

    #[test]
    fn test_detects_ace_token() {
        let content = "ace_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";
        let result = scan(content);
        assert!(result.has_sensitive);
        assert_eq!(result.findings[0].label, "ace_token");
    }

    #[test]
    fn test_detects_github_oauth_token() {
        let content = "oauth = \"gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\"";
        let result = scan(content);
        assert!(result.has_sensitive);
        assert_eq!(result.findings[0].label, "github_oauth");
    }

    #[test]
    fn test_detects_bearer_token() {
        let content = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = scan(content);
        assert!(result.has_sensitive);
        assert!(result.findings.iter().any(|f| f.label == "bearer_token"));
    }

    #[test]
    fn test_detects_base64_secret() {
        let content = "secret = \"dGhpc0lzQVZlcnlMb25nQmFzZTY0U2VjcmV0S2V5VGhhdFNob3VsZEJlRGV0ZWN0ZWQ=\"";
        let result = scan(content);
        assert!(result.has_sensitive);
        assert!(result.findings.iter().any(|f| f.label == "base64_secret"));
    }

    #[test]
    fn test_scan_empty_content() {
        let result = scan("");
        assert!(!result.has_sensitive);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn test_scan_reports_correct_line_numbers() {
        let content = "line one\nsk-abc123def456ghi789jkl012mno345pqr678\nline three";
        let result = scan(content);
        assert!(result.has_sensitive);
        assert_eq!(result.findings[0].line, 2);
    }

    #[test]
    fn test_redact_multiple_patterns() {
        let content = "api = \"sk-abc123def456ghi789jkl012\"\noauth = \"gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\"";
        let redacted = redact(content);
        assert!(redacted.contains("<REDACTED:api_key>"));
        assert!(redacted.contains("<REDACTED:github_oauth>"));
        assert!(!redacted.contains("sk-abc123"));
        assert!(!redacted.contains("gho_"));
    }

    #[test]
    fn test_redact_preserves_clean_content() {
        let content = "model = \"gpt-4\"\ntemperature = 0.7";
        assert_eq!(redact(content), content);
    }
}
