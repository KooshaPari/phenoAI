//! HTTP delegate design — router revamp forwards `/v1/*` to cliproxy++ (Go plane).

/// OpenAI-compatible paths delegated to cliproxy++.
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
pub const MODELS_PATH: &str = "/v1/models";

/// Resolved upstream target for a combo route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateRequest {
    pub target: String,
    pub path: &'static str,
    pub variant: super::ComboVariant,
}

/// Build cliproxy delegate URL from base + combo variant.
pub fn build_delegate_request(
    cliproxy_base: &str,
    model_id: &str,
) -> Option<DelegateRequest> {
    let variant = super::ComboVariant::parse(model_id)?;
    let _profile = variant.delegate_target();
    let base = cliproxy_base.trim_end_matches('/');
    Some(DelegateRequest {
        target: format!("{base}{CHAT_COMPLETIONS_PATH}"),
        path: CHAT_COMPLETIONS_PATH,
        variant,
    })
}

/// Map internal delegate label to cliproxy scoring profile query param.
pub fn scoring_profile(target: &str) -> Option<&'static str> {
    match target {
        "cliproxy-delegate-quality" => Some("quality"),
        "cliproxy-delegate-latency" => Some("latency"),
        "cliproxy-delegate-cost" => Some("cost"),
        "cliproxy-delegate-quota" => Some("quota"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_chat_completions_delegate_url() {
        let req = build_delegate_request("http://127.0.0.1:8317", "auto/coding").unwrap();
        assert_eq!(req.target, "http://127.0.0.1:8317/v1/chat/completions");
        assert_eq!(req.path, CHAT_COMPLETIONS_PATH);
        assert_eq!(req.variant, super::super::ComboVariant::Coding);
    }

    #[test]
    fn scoring_profiles_map() {
        assert_eq!(scoring_profile("cliproxy-delegate-latency"), Some("latency"));
        assert_eq!(scoring_profile("unknown"), None);
    }
}
