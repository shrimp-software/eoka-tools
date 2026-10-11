use eoka::{BoundedFetchRequest, Page};
use eoka_protocol::{ErrorDetail, FetchArgs, Response};

pub async fn fetch(page: &Page, args: FetchArgs) -> Response {
    let mut request = BoundedFetchRequest::get(args.url);
    request.body = args.body;
    if let Some(method) = args.method {
        request.method = method;
    }
    if let Some(redirect) = args.redirect {
        request.redirect = redirect;
    }
    if let Some(max_bytes) = args.max_body {
        request.max_bytes = max_bytes;
    }
    if let Some(headers) = args.headers {
        match serde_json::from_value(headers) {
            Ok(headers) => request.headers = headers,
            Err(_) => {
                return Response::err_detail(ErrorDetail::new(
                    "invalid_input",
                    "Headers must be an object of string values",
                ))
            }
        }
    }
    let result = match page.fetch_bounded(&request).await {
        Ok(result) => result,
        Err(error) => {
            return Response::err_detail(
                ErrorDetail::new("fetch_failed", error.to_string()).retryable(false),
            )
        }
    };
    if args.body_only {
        if result.body_omitted.as_deref() == Some("binary") {
            return Response::err_detail(ErrorDetail::new(
                "fetch_binary_body",
                "Binary body omitted; use eoka download --url or --click with --out",
            ));
        }
        if !result.truncated {
            return Response::ok_text(result.body);
        }
    }
    let value = match serde_json::to_value(result) {
        Ok(value) => value,
        Err(error) => return Response::err(error.to_string()),
    };
    if args.body_only {
        let mut response = Response::err_detail(ErrorDetail::new(
            "fetch_body_truncated",
            "Body-only output exceeded max_body; increase the bounded limit or inspect JSON output",
        ));
        response.data = Some(value);
        return response;
    }
    Response::ok(value)
}
