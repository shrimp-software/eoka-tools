use eoka_protocol::{ErrorDetail, OperationSpec, Response};

pub fn built(feature: &str) -> bool {
    (feature == "anti-captcha" && cfg!(feature = "anti-captcha"))
        || (feature == "datadome" && cfg!(feature = "datadome"))
}

pub fn operation_built(operation: &OperationSpec) -> bool {
    operation.feature.is_none_or(built)
}

pub fn method_built(method: &str) -> bool {
    eoka_protocol::sidecar::method_feature(method).is_none_or(built)
}

pub fn unavailable_message(feature: &str) -> String {
    format!(
        "this eoka build does not include the {feature} plugin; rebuild with --features {feature}"
    )
}

pub fn unavailable(feature: &str) -> Response {
    Response::err_detail(
        ErrorDetail::new("feature_unavailable", unavailable_message(feature)).retryable(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_operations_follow_the_build() {
        assert_eq!(method_built("captcha.datadome"), cfg!(feature = "datadome"));
        assert_eq!(
            method_built("page.solve_captcha"),
            cfg!(feature = "anti-captcha")
        );
        assert!(method_built("captcha.turnstile"));
        assert!(method_built("page.title"));
        assert!(!built("unknown"));
    }

    #[test]
    fn a_missing_plugin_names_its_feature() {
        let response = unavailable("datadome");
        assert!(!response.ok);
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "feature_unavailable");
        assert!(detail.message.contains("--features datadome"));
    }
}
