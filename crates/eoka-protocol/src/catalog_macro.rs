//! `define_operations!` builds the operation catalog: the `Request` enum,
//! its command names and tool metadata, from one table in `catalog.rs`.

macro_rules! define_operations {
    (
        $(
            $variant:ident {
                path: $path:literal,
                cmd: $cmd:literal,
                name: $name:literal,
                description: $description:literal,
                capability: $capability:ident,
                exposure: $exposure:ident,
                read_only: $read_only:literal,
                destructive: $destructive:literal
                $(, input: $input:ty)?
                $(,)?
            }
        ),+ $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum OperationId {
            $($variant),+
        }

        #[derive(Debug, Clone, Copy)]
        pub struct OperationSpec {
            pub id: OperationId,
            pub path: &'static str,
            pub cmd: &'static str,
            pub name: &'static str,
            pub description: &'static str,
            pub capability: OperationCapability,
            pub exposure: OperationExposure,
            pub read_only: bool,
            pub destructive: bool,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(tag = "cmd", content = "args")]
        pub enum Request {
            $(
                #[serde(rename = $cmd)]
                $variant$(($input))?,
            )+
        }

        pub const OPERATIONS: &[OperationSpec] = &[
            $(OperationSpec {
                id: OperationId::$variant,
                path: $path,
                cmd: $cmd,
                name: $name,
                description: $description,
                capability: OperationCapability::$capability,
                exposure: OperationExposure::$exposure,
                read_only: $read_only,
                destructive: $destructive,
            }),+
        ];
        pub fn all_operations() -> &'static [OperationSpec] {
            OPERATIONS
        }

        pub fn operation_by_path(path: &str) -> Option<&'static OperationSpec> {
            OPERATIONS.iter().find(|operation| operation.path == path)
        }

        pub fn operation_by_cmd(cmd: &str) -> Option<&'static OperationSpec> {
            OPERATIONS.iter().find(|operation| operation.cmd == cmd)
        }

        pub fn default_agent_operations() -> impl Iterator<Item = &'static OperationSpec> {
            OPERATIONS
                .iter()
                .filter(|operation| operation.exposure == OperationExposure::DefaultAgent)
        }

        pub fn exposed_operations(
            include_opt_in: bool,
        ) -> impl Iterator<Item = &'static OperationSpec> {
            OPERATIONS.iter().filter(move |operation| {
                operation.exposure == OperationExposure::DefaultAgent
                    || (include_opt_in && operation.exposure == OperationExposure::OptIn)
            })
        }

        pub fn request_from_operation_path(
            path: &str,
            input: serde_json::Value,
        ) -> Result<Request, String> {
            let operation = operation_by_path(path)
                .ok_or_else(|| format!("unknown eoka operation path: {path}"))?;
            request_from_cmd(operation.cmd, input)
        }

        pub fn request_from_cmd(cmd: &str, input: serde_json::Value) -> Result<Request, String> {
            match cmd {
                $(
                    $cmd => Ok(define_operations!(@request Request::$variant, input $(, $input)?)),
                )+
                _ => Err(format!("unknown eoka protocol command: {cmd}")),
            }
        }

        pub fn input_schema_for_cmd(cmd: &str) -> serde_json::Value {
            match cmd {
                $(
                    $cmd => define_operations!(@schema $( $input )?),
                )+
                _ => serde_json::json!({ "type": "object", "additionalProperties": false }),
            }
        }

        pub fn input_schema_for_operation(operation: &OperationSpec) -> serde_json::Value {
            input_schema_for_cmd(operation.cmd)
        }

        pub fn tags_for_operation(operation: &OperationSpec) -> Vec<&'static str> {
            vec!["eoka", operation.capability.as_str()]
        }

        pub fn manifest_entry_for_operation(
            namespace: &str,
            operation: &OperationSpec,
        ) -> ToolManifestEntry {
            ToolManifestEntry {
                path: format!("{}.{}", namespace, operation.path),
                cmd: operation.cmd,
                name: operation.name,
                description: operation.description,
                capability: operation.capability.as_str(),
                exposure: operation.exposure.as_str(),
                read_only: operation.read_only,
                destructive: operation.destructive,
                input_schema: input_schema_for_operation(operation),
                tags: tags_for_operation(operation),
            }
        }

        pub fn manifest_for_operations(
            namespace: &str,
            include_opt_in: bool,
        ) -> Vec<ToolManifestEntry> {
            exposed_operations(include_opt_in)
                .map(|operation| manifest_entry_for_operation(namespace, operation))
                .collect()
        }

        impl Request {
            pub fn cmd(&self) -> &'static str {
                match self {
                    $(
                        define_operations!(@match_variant Self::$variant $(, $input)?) => $cmd,
                    )+
                }
            }

            pub fn args_json(&self) -> serde_json::Value {
                serde_json::to_value(self)
                    .ok()
                    .and_then(|value| value.get("args").cloned())
                    .unwrap_or_else(|| serde_json::json!({}))
            }
        }
    };

    (@request $variant:path, $input:ident, $ty:ty) => {{
        serde_json::from_value::<$ty>($input)
            .map($variant)
            .map_err(|error| error.to_string())?
    }};

    (@request $variant:path, $input:ident) => {{
        if $input != serde_json::json!({}) && $input != serde_json::Value::Null {
            return Err("expected empty object for zero-argument command".to_string());
        }
        $variant
    }};

    (@schema $ty:ty) => {
        schema_for::<$ty>()
    };

    (@schema) => {
        serde_json::json!({ "type": "object", "additionalProperties": false })
    };

    (@match_variant $variant:path, $ty:ty) => {
        $variant(_)
    };

    (@match_variant $variant:path) => {
        $variant
    };

}
