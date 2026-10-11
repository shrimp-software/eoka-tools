use crate::protocol::Response;

pub fn print_response(response: &Response, json_mode: bool) {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(response).unwrap_or_default()
        );
        return;
    }

    if !response.ok {
        eprint!("{}", format_error(response));
        return;
    }

    if let Some(ref data) = response.data {
        match data {
            serde_json::Value::String(s) => println!("{}", s),
            serde_json::Value::Object(obj) if obj.get("text").is_some() => {
                if let Some(text) = obj.get("text").and_then(|value| value.as_str()) {
                    println!("{}", text);
                }
            }
            other => println!(
                "{}",
                serde_json::to_string_pretty(other).unwrap_or_default()
            ),
        }
    }
}

fn format_error(response: &Response) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    let detail = response.error_detail.as_ref();
    let _ = writeln!(
        text,
        "Error [{}]: {}",
        detail.map_or("eoka_error", |d| &d.code),
        response.error.as_deref().unwrap_or("Command failed")
    );
    if let Some(context) = detail.and_then(|detail| detail.context.as_ref()) {
        if let Some(action) = context.action_status {
            text.push_str(match action {
                eoka_protocol::ActionStatus::NotAttempted => "Action was not attempted.\n",
                eoka_protocol::ActionStatus::MayHaveExecuted => {
                    "Action may have executed; it was not replayed.\n"
                }
            });
        }
        if let Some(page) = &context.page {
            text.push_str(&eoka_server::page_context::format(page));
        }
    }
    let data = response.data.as_ref();
    let id = data.and_then(|data| data["id"].as_str()).or_else(|| {
        detail
            .and_then(|detail| detail.context.as_ref())
            .and_then(|context| context.download_id.as_deref())
    });
    let session = response
        .meta
        .as_ref()
        .and_then(|meta| meta.session.as_deref())
        .unwrap_or("default");
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\"'\"'"));
    if let Some(id) = id {
        let _ = writeln!(text, "Download job: {id}");
        if let Some(data) = data {
            for key in ["state", "output", "published_path", "staging_path"] {
                if let Some(value) = data[key].as_str() {
                    let _ = writeln!(text, "{key}: {value}");
                }
            }
        } else {
            text.push_str("Current job state unavailable.\n");
        }
        let _ = writeln!(
            text,
            "Next: eoka --session {} download status {}",
            quote(session),
            quote(id)
        );
    }
    if id.is_none()
        && detail
            .and_then(|detail| detail.context.as_ref())
            .and_then(|context| context.page.as_ref())
            .is_some()
    {
        let _ = writeln!(
            text,
            "Next: eoka --session {} --agent observe",
            quote(session)
        );
    }
    if let Some(hint) = detail.and_then(|detail| detail.hint.as_deref()) {
        let _ = writeln!(text, "Hint: {hint}");
    }
    text
}
