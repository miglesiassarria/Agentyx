
    LineOutcome::Continue
}

/// Heuristic: a request error is "retryable" if it is a network
/// error (connection refused, timeout, DNS). Anything that looks
/// like an HTTP-status error from upstream is handled by the
/// caller; this helper only covers the transport layer.
fn is_retryable(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_handles_tool_call() {
        let mut started = false;
        // Prime with a first chunk so `started` becomes true and
        // the second chunk is not interpreted as MessageStart.
        let prime =
            r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":""},"done":false}"#;
        let _ = parse_ollama_line(prime, &mut started, "llama3.1:8b");
        assert!(started);
        let line = r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":"","tool_calls":[{"id":"tc-1","function":{"name":"read_file","arguments":{"path":"foo.txt"}}}]},"done":false}"#;
        match parse_ollama_line(line, &mut started, "llama3.1:8b") {
            LineOutcome::PendingTool { name, args, .. } => {
                assert_eq!(name, "read_file");
                assert_eq!(args["path"], "foo.txt");
            }
            other => panic!("expected PendingTool, got {other:?}"),
        }
    }

    #[test]
    fn parse_handles_malformed_line_as_continue() {
        let mut started = false;
        let line = "not json";
        let outcome = parse_ollama_line(line, &mut started, "x");
        assert!(matches!(outcome, LineOutcome::Continue));
    }

    #[test]
    fn capabilities_for_llama3_includes_tools() {
        let p = OllamaProvider::new().unwrap();
        let caps = p.capabilities("llama3.1:8b");
        assert!(caps.tools);
        assert!(!caps.vision);
    }

    #[test]
    fn capabilities_for_unknown_model_is_conservative() {
        let p = OllamaProvider::new().unwrap();
        let caps = p.capabilities("totally-unknown-7b");
        assert!(!caps.tools);
    }
}
