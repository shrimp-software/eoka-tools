use super::*;

impl EokaServer {
    pub(super) async fn action_tool(
        &self,
        tool: &str,
        command: &str,
        mut arguments: Value,
    ) -> Result<CallToolResult, ErrorData> {
        let operation_id = match arguments["operation_id"].as_str() {
            Some(id) => id.to_owned(),
            None => new_operation_id(),
        };
        let unverified = runs_unverified(command, &arguments);
        if unverified {
            arguments["exploratory"] = Value::Bool(true);
        }
        let request = if command == "action_perform" {
            arguments["operation_id"] = Value::String(operation_id);
            eoka_protocol::request_from_cmd(command, arguments).map_err(invalid)?
        } else {
            Operation::ActionCommand(eoka_protocol::ActionCommandArgs {
                operation_id,
                command: command.to_owned(),
                arguments,
            })
        };
        let (response, _) = self.respond(tool, request, Needs::Page).await?;
        protocol_tool_response(if unverified {
            as_unverified_success(response)
        } else {
            with_action_hint(response)
        })
    }
}

const EXPECT_REQUIRED_HINT: &str = "Add expect with what the action should change, for example {\"kind\":\"url\",\"value\":\"/submit\",\"contains\":true}. If the result is unknown, use the click tool without expect.";
const EXPECT_SATISFIED_HINT: &str = "This expect was already true before the action, so it cannot prove the action worked. Use a condition that is false now, or omit expect if the result is unknown.";
const UNVERIFIED_NOTE: &str = "The action ran, but its result was not verified because no expect was given. Read the page with page_text or observe to see what happened. Do not repeat the action unless the page shows it failed.";

fn runs_unverified(cmd: &str, args: &Value) -> bool {
    matches!(cmd, "click" | "dblclick" | "key") && args["expect"].is_null()
}

fn as_unverified_success(mut response: eoka_protocol::Response) -> eoka_protocol::Response {
    let dispatched = response
        .data
        .as_ref()
        .is_some_and(|data| data["dispatch_intent"] == true && data["status"] == "outcome_unknown");
    if dispatched {
        response.ok = true;
        response.error = None;
        response.error_detail = None;
        if let Some(data) = response.data.as_mut() {
            data["note"] = Value::String(UNVERIFIED_NOTE.into());
        }
    }
    response
}

pub(super) fn new_operation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn action_hint(message: &str) -> Option<&'static str> {
    if message.contains("expectation_required") {
        Some(EXPECT_REQUIRED_HINT)
    } else if message.contains("expectation_already_satisfied") {
        Some(EXPECT_SATISFIED_HINT)
    } else {
        None
    }
}

fn with_action_hint(mut response: eoka_protocol::Response) -> eoka_protocol::Response {
    let Some(hint) = response.error.as_deref().and_then(action_hint) else {
        return response;
    };
    if let Some(error) = response.error.as_mut() {
        error.push(' ');
        error.push_str(hint);
    }
    if let Some(detail) = response.error_detail.as_mut() {
        detail.message.push(' ');
        detail.message.push_str(hint);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_operation_ids_are_uuids_the_server_accepts() {
        assert!(uuid::Uuid::parse_str(&new_operation_id()).is_ok());
        assert_ne!(new_operation_id(), new_operation_id());
    }

    #[test]
    fn expectation_errors_gain_a_usable_hint() {
        let mut response = eoka_protocol::Response::err("CDP error: expectation_required: x");
        response = with_action_hint(response);
        assert!(response.error.unwrap().contains("without expect"));
        let satisfied = with_action_hint(eoka_protocol::Response::err(
            "CDP error: expectation_already_satisfied: x",
        ));
        assert!(satisfied.error.unwrap().contains("already true"));
        let other = with_action_hint(eoka_protocol::Response::err("other"));
        assert_eq!(other.error.as_deref(), Some("other"));
    }

    #[test]
    fn only_clicks_and_keys_without_expect_run_unverified() {
        let bare = serde_json::json!({"target":"Join"});
        assert!(runs_unverified("click", &bare));
        assert!(runs_unverified("key", &bare));
        assert!(!runs_unverified("fill", &bare));
        let expected = serde_json::json!({"target":"Join","expect":{"kind":"document_ready"}});
        assert!(!runs_unverified("click", &expected));
    }

    #[test]
    fn only_a_dispatched_unknown_outcome_counts_as_unverified_success() {
        let response = |status: &str, dispatched: bool| {
            let mut response = eoka_protocol::Response::err("x");
            response.data = Some(serde_json::json!({"status":status,"dispatch_intent":dispatched}));
            as_unverified_success(response)
        };
        let unknown = response("outcome_unknown", true);
        assert!(unknown.ok);
        assert_eq!(unknown.data.unwrap()["status"], "outcome_unknown");
        assert!(!response("not_dispatched", false).ok);
        assert!(!response("outcome_unknown", false).ok);
    }

    #[test]
    fn the_hint_example_is_a_valid_expectation() {
        let start = EXPECT_REQUIRED_HINT.find('{').unwrap();
        let end = EXPECT_REQUIRED_HINT.find('}').unwrap();
        let parsed: eoka_protocol::Expectation =
            serde_json::from_str(&EXPECT_REQUIRED_HINT[start..=end]).unwrap();
        assert!(matches!(
            parsed,
            eoka_protocol::Expectation::Url { contains: true, .. }
        ));
    }
}
