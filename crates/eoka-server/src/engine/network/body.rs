use base64::Engine;
use eoka_protocol::{NetworkBody, NetworkBodySummary};
use serde_json::Value;

use super::BodyCapture;

pub(super) trait BodyCaptureExt {
    fn omitted(reason: impl Into<String>) -> Self;
    fn len(&self) -> usize;
    fn to_text(&self) -> Option<String>;
}

impl BodyCaptureExt for BodyCapture {
    fn omitted(reason: impl Into<String>) -> Self {
        Self {
            bytes: Vec::new(),
            base64_encoded: false,
            mime_type: None,
            omitted: Some(reason.into()),
        }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn to_text(&self) -> Option<String> {
        if self.omitted.is_some() {
            return None;
        }
        if self.base64_encoded {
            return Some(base64::engine::general_purpose::STANDARD.encode(&self.bytes));
        }
        String::from_utf8(self.bytes.clone()).ok()
    }
}

pub(super) fn capture_text_body(data: &str, limit: usize, enabled: bool) -> BodyCapture {
    if !enabled {
        return BodyCapture::omitted("disabled");
    }
    if data.len() > limit {
        return BodyCapture::omitted("too large");
    }
    BodyCapture {
        bytes: data.as_bytes().to_vec(),
        base64_encoded: false,
        mime_type: None,
        omitted: None,
    }
}

pub(super) fn decode_response_body(value: Value, limit: usize) -> BodyCapture {
    let Some(text) = value.get("body").and_then(Value::as_str) else {
        return BodyCapture::omitted("empty");
    };
    let encoded = value
        .get("base64Encoded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let bytes = if encoded {
        match base64::engine::general_purpose::STANDARD.decode(text) {
            Ok(bytes) => bytes,
            Err(error) => return BodyCapture::omitted(format!("decode failed: {}", error)),
        }
    } else {
        text.as_bytes().to_vec()
    };
    if bytes.len() > limit {
        return BodyCapture::omitted("too large");
    }
    BodyCapture {
        bytes,
        base64_encoded: encoded,
        mime_type: None,
        omitted: None,
    }
}

pub(super) fn body_summary(body: Option<&BodyCapture>) -> Option<NetworkBodySummary> {
    body.map(|body| NetworkBodySummary {
        bytes: body.len(),
        base64: body.base64_encoded,
        omitted: body.omitted.clone(),
    })
}

pub(super) fn body_json(body: Option<&BodyCapture>) -> Option<NetworkBody> {
    body.map(|body| NetworkBody {
        bytes: body.len(),
        base64: body.base64_encoded,
        text: body.to_text(),
        omitted: body.omitted.clone(),
        mime_type: body.mime_type.clone(),
        truncated: None,
    })
}

pub(super) fn limit_body(body: &mut NetworkBody, max_body: usize) {
    let Some(text) = body.text.as_mut() else {
        return;
    };
    if text.len() <= max_body {
        return;
    }
    *text = text.chars().take(max_body).collect();
    body.truncated = Some(true);
}
