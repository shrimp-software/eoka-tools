use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

use eoka_protocol::{input_schema_for_cmd, operation_by_cmd, OperationExposure, OperationSpec};

use super::EokaServer;

#[derive(Clone, Copy)]
pub(super) struct Alias {
    tool: &'static str,
    cmd: &'static str,
    about: Option<&'static str>,
    fixed: &'static str,
    defaults: &'static str,
    keep: Option<&'static [&'static str]>,
    hides: bool,
}

const fn rename(tool: &'static str, cmd: &'static str) -> Alias {
    Alias {
        tool,
        cmd,
        about: None,
        fixed: "{}",
        defaults: "{}",
        keep: None,
        hides: true,
    }
}

const fn variant(tool: &'static str, cmd: &'static str, about: &'static str) -> Alias {
    Alias {
        about: Some(about),
        hides: false,
        ..rename(tool, cmd)
    }
}

impl Alias {
    const fn fixed(self, fixed: &'static str) -> Self {
        Self { fixed, ..self }
    }

    const fn defaults(self, defaults: &'static str) -> Self {
        Self { defaults, ..self }
    }

    const fn keep(self, keep: &'static [&'static str]) -> Self {
        Self {
            keep: Some(keep),
            ..self
        }
    }

    const fn hiding(self) -> Self {
        Self {
            hides: true,
            ..self
        }
    }

    fn schema(&self) -> Value {
        let mut schema = input_schema_for_cmd(self.cmd);
        let fixed = object(self.fixed);
        let shown = |name: &str| {
            !fixed.contains_key(name) && self.keep.is_none_or(|keep| keep.contains(&name))
        };
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            properties.retain(|name, _| shown(name));
        }
        if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|name| name.as_str().is_some_and(shown));
        }
        without_unused_definitions(schema)
    }
}

pub(super) const ALIASES: &[Alias] = &[
    rename("navigate", "open"),
    rename("page_info", "info"),
    rename("page_text", "text"),
    rename("extract", "eval"),
    rename("list_tabs", "tab_list"),
    rename("new_tab", "tab_new"),
    rename("switch_tab", "tab_switch"),
    rename("close_tab", "tab_close"),
    rename("type_key", "key"),
    rename("find_text", "find"),
    rename("accept_dialog", "dialog_accept"),
    rename("dismiss_dialog", "dialog_dismiss"),
    rename("detect_captcha", "captcha_detect"),
    rename("inject_captcha_token", "captcha_inject"),
    rename("captcha_turnstile", "captcha_turnstile"),
    rename("batch", "action_batch"),
    rename("history_go", "history_go"),
    rename("close", "close"),
    variant(
        "local_storage_get",
        "storage",
        "Get a value from localStorage. Returns null when the key is not set.",
    )
    .fixed(r#"{"session_storage":false}"#)
    .hiding(),
    variant("local_storage_set", "set_storage", "Set a value in localStorage.")
        .fixed(r#"{"session_storage":false}"#)
        .hiding(),
    variant(
        "session_storage_get",
        "storage",
        "Get a value from sessionStorage. Returns null when the key is not set.",
    )
    .fixed(r#"{"session_storage":true}"#)
    .hiding(),
    variant("session_storage_set", "set_storage", "Set a value in sessionStorage.")
        .fixed(r#"{"session_storage":true}"#)
        .hiding(),
    variant(
        "wait_ms",
        "wait",
        "Wait for a number of milliseconds, at most 30000, between timing-sensitive actions.",
    )
    .keep(&["ms"]),
    variant(
        "wait_for_text",
        "wait",
        "Wait for text to appear on the page (case-insensitive). Errors on timeout.",
    )
    .keep(&["text", "timeout"]),
    variant(
        "wait_for_network_idle",
        "wait",
        "Wait for network to go idle (no XHR/fetch for network_idle ms, 500 by default). Useful after actions that trigger API calls before reading the result.",
    )
    .keep(&["network_idle", "timeout"])
    .defaults(r#"{"network_idle":500}"#),
    variant(
        "screenshot",
        "screenshot",
        "Take annotated screenshot with numbered element boxes. Returns PNG image AND element list. Best way to see the page.",
    )
    .fixed(r#"{"annotate":true,"inline":true}"#)
    .keep(&[]),
    variant(
        "click_intercept_nav",
        "click",
        "Install a page navigation diagnostic and admit a durable click. Requires an explicit expect condition. Returns the operation record; wait with operation_wait, then inspect window.__eoka_nav using extract. Intercepts history.pushState/replaceState; location navigation cannot be reliably intercepted by page JavaScript. intercept_nav=allow preserves history navigation.",
    )
    .keep(&["target", "operation_id", "expect", "timeout_ms", "intercept_nav"])
    .defaults(r#"{"intercept_nav":"block"}"#),
];

fn without_unused_definitions(mut schema: Value) -> Value {
    let Some(definitions) = schema
        .get_mut("$defs")
        .and_then(Value::as_object_mut)
        .map(std::mem::take)
    else {
        return schema;
    };
    let mut used = JsonObject::new();
    let mut pending = vec![schema["properties"].to_string()];
    while let Some(text) = pending.pop() {
        for (name, definition) in &definitions {
            if !used.contains_key(name) && text.contains(&format!("\"#/$defs/{name}\"")) {
                used.insert(name.clone(), definition.clone());
                pending.push(definition.to_string());
            }
        }
    }
    if used.is_empty() {
        schema.as_object_mut().map(|schema| schema.remove("$defs"));
    } else {
        schema["$defs"] = Value::Object(used);
    }
    schema
}

fn object(json: &str) -> JsonObject {
    serde_json::from_str(json).unwrap_or_default()
}

fn tool_name(operation: &OperationSpec) -> String {
    operation.name.replace('.', "_")
}

fn hidden(cmd: &str) -> bool {
    ALIASES.iter().any(|alias| alias.hides && alias.cmd == cmd)
}

#[derive(Clone, Copy)]
pub(super) struct Route {
    pub(super) operation: &'static OperationSpec,
    alias: Option<&'static Alias>,
}

impl Route {
    pub(super) fn unknown_argument(&self, given: &Value) -> Option<String> {
        let schema = match self.alias {
            Some(alias) => alias.schema(),
            None => input_schema_for_cmd(self.operation.cmd),
        };
        let known = schema["properties"].as_object()?;
        given
            .as_object()?
            .keys()
            .find(|key| !known.contains_key(*key))
            .map(|key| {
                let mut accepted: Vec<&str> = known.keys().map(String::as_str).collect();
                accepted.sort_unstable();
                format!(
                    "unknown argument `{key}`; accepted: {}",
                    accepted.join(", ")
                )
            })
    }

    pub(super) fn arguments(&self, given: &Value) -> Value {
        let mut arguments = given.as_object().cloned().unwrap_or_default();
        if let Some(alias) = self.alias {
            for (name, value) in object(alias.defaults) {
                arguments.entry(name).or_insert(value);
            }
            arguments.extend(object(alias.fixed));
        }
        Value::Object(arguments)
    }
}

pub(super) fn route(tool: &str) -> Option<Route> {
    if let Some(alias) = ALIASES.iter().find(|alias| alias.tool == tool) {
        return Some(Route {
            operation: operation_by_cmd(alias.cmd)?,
            alias: Some(alias),
        });
    }
    eoka_protocol::sidecar::page_operations()
        .find(|operation| tool_name(operation) == tool && !hidden(operation.cmd))
        .map(|operation| Route {
            operation,
            alias: None,
        })
}

fn add(
    router: &mut ToolRouter<EokaServer>,
    name: String,
    route: Route,
    about: &str,
    schema: Value,
) {
    let operation = route.operation;
    let mut tool = Tool::new(name.clone(), about.to_owned(), rmcp::model::object(schema));
    tool.annotations = Some(
        ToolAnnotations::new()
            .read_only(operation.read_only)
            .destructive(operation.destructive),
    );
    router.add_route(ToolRoute::new_dyn(
        tool,
        move |context: ToolCallContext<'_, EokaServer>| {
            let name = name.clone();
            Box::pin(async move {
                context
                    .service
                    .catalog_tool(&name, route, context.arguments)
                    .await
                    .map(Into::into)
            })
        },
    ));
}

pub(super) fn router(include_opt_in: bool) -> ToolRouter<EokaServer> {
    let mut router = ToolRouter::new();
    for alias in ALIASES {
        let operation =
            operation_by_cmd(alias.cmd).expect("an MCP alias names a cataloged command");
        if !eoka_server::features::operation_built(operation) {
            continue;
        }
        let route = Route {
            operation,
            alias: Some(alias),
        };
        let about = alias.about.unwrap_or(operation.description);
        add(
            &mut router,
            alias.tool.to_owned(),
            route,
            about,
            alias.schema(),
        );
    }
    for operation in eoka_protocol::sidecar::page_operations() {
        let name = tool_name(operation);
        let opted_out = operation.exposure == OperationExposure::OptIn && !include_opt_in;
        if opted_out
            || !eoka_server::features::operation_built(operation)
            || hidden(operation.cmd)
            || router.map.contains_key(name.as_str())
        {
            continue;
        }
        let route = Route {
            operation,
            alias: None,
        };
        add(
            &mut router,
            name,
            route,
            operation.description,
            input_schema_for_cmd(operation.cmd),
        );
    }
    #[cfg(feature = "anti-captcha")]
    super::solve_captcha::register(&mut router);
    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renamed_or_unknown_arguments_are_refused_instead_of_ignored() {
        let load_state = route("load_state").unwrap();
        assert!(load_state
            .unknown_argument(&serde_json::json!({"path": "state.json", "navigate": false}))
            .is_some_and(|error| error.contains("`navigate`")));
        assert!(load_state
            .unknown_argument(&serde_json::json!({"path": "state.json", "no_navigate": true}))
            .is_none());
        let wait = route("wait_for_text").unwrap();
        assert!(wait
            .unknown_argument(&serde_json::json!({"text": "x", "timeout_ms": 1}))
            .is_some());
    }

    fn plain(alias: &Alias) -> bool {
        alias.about.is_none() && alias.keep.is_none() && object(alias.fixed).is_empty()
    }

    #[test]
    fn every_catalog_operation_is_an_mcp_tool() {
        let every = router(true);
        for alias in ALIASES {
            let built =
                eoka_server::features::operation_built(operation_by_cmd(alias.cmd).unwrap());
            assert_eq!(every.map.contains_key(alias.tool), built, "{}", alias.tool);
        }
        for operation in eoka_protocol::sidecar::page_operations() {
            assert!(
                hidden(operation.cmd)
                    || every.map.contains_key(tool_name(operation).as_str())
                    || !eoka_server::features::operation_built(operation),
                "catalog operation {} has no MCP tool",
                operation.cmd
            );
        }
        let default = router(false);
        assert!(default.map.contains_key("content"));
        assert!(default.map.contains_key("reload"));
        assert!(default.map.contains_key("wait"));
        for covered in [
            "open",
            "eval",
            "info",
            "text",
            "storage",
            "set_storage",
            "tab_list",
        ] {
            assert!(!every.map.contains_key(covered), "{covered}");
        }
        for opt_in in [
            "click_shadow",
            "recording_start",
            "clone_from",
            "fake_camera",
        ] {
            assert!(!default.map.contains_key(opt_in), "{opt_in}");
            assert!(every.map.contains_key(opt_in), "{opt_in}");
        }
    }

    #[test]
    fn mcp_protocol_descriptors_match_catalog() {
        let every = router(true);
        let catalog = ALIASES
            .iter()
            .filter(|alias| plain(alias))
            .map(|alias| (alias.tool.to_owned(), alias.cmd))
            .chain(
                eoka_protocol::sidecar::page_operations()
                    .filter(|operation| !hidden(operation.cmd))
                    .map(|operation| (tool_name(operation), operation.cmd))
                    .filter(|(tool, _)| !ALIASES.iter().any(|alias| alias.tool == tool)),
            );
        for (tool, cmd) in catalog {
            let Some(route) = every.get(&tool) else {
                continue;
            };
            let operation = operation_by_cmd(cmd).unwrap();
            assert_eq!(
                route.description.as_deref(),
                Some(operation.description),
                "{tool}"
            );
            assert_eq!(
                serde_json::to_value(&*route.input_schema).unwrap(),
                input_schema_for_cmd(cmd),
                "{tool}"
            );
        }
        for (tool, route) in every
            .map
            .iter()
            .filter(|(tool, _)| *tool != "solve_captcha")
        {
            let operation = super::route(tool).unwrap().operation;
            let annotations = route.attr.annotations.as_ref().unwrap();
            assert_eq!(
                annotations.read_only_hint,
                Some(operation.read_only),
                "{tool}"
            );
            assert_eq!(
                annotations.destructive_hint,
                Some(operation.destructive),
                "{tool}"
            );
        }
    }

    #[test]
    fn a_variant_fixes_its_flags_and_shows_only_what_it_keeps() {
        let get = route("session_storage_get").unwrap();
        assert_eq!(
            get.arguments(&serde_json::json!({"key": "k", "session_storage": false})),
            serde_json::json!({"key": "k", "session_storage": true})
        );
        let idle = route("wait_for_network_idle").unwrap();
        assert_eq!(
            idle.arguments(&Value::Null),
            serde_json::json!({"network_idle": 500})
        );
        assert_eq!(
            idle.arguments(&serde_json::json!({"network_idle": 100}))["network_idle"],
            100
        );
        let screenshot = ALIASES
            .iter()
            .find(|alias| alias.tool == "screenshot")
            .unwrap();
        assert_eq!(screenshot.schema()["properties"], serde_json::json!({}));
        let storage = ALIASES
            .iter()
            .find(|alias| alias.tool == "local_storage_set")
            .unwrap()
            .schema();
        assert!(storage["properties"].get("session_storage").is_none());
        assert_eq!(storage["required"], serde_json::json!(["key", "value"]));
    }
}
