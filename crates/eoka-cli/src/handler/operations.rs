use super::*;
use eoka::{LocatorQuery, LocatorSpec};
use eoka_server::{
    operations::{commands, OperationManager},
    Target,
};

impl Handler {
    pub fn operation_manager(&self) -> OperationManager {
        self.operations.clone()
    }

    pub(super) async fn handle_action(&mut self, cmd: &str, args: &Value) -> Response {
        let prepared: anyhow::Result<_> = async {
            let tab = self.require_tab().map_err(anyhow::Error::msg)?;
            if cmd == "action_perform" {
                let request: eoka_protocol::ActionPerformArgs =
                    serde_json::from_value(args.clone())?;
                return Ok((
                    tab.page.clone(),
                    request.operation_id,
                    request.action,
                    request.options,
                ));
            }
            let id = args["operation_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("operation_id is required; upgrade the client"))?
                .to_owned();
            let target = if let Some(target) = args["target"].as_str() {
                Some(match Target::parse(target) {
                    Target::Ref(label) => LocatorSpec {
                        query: LocatorQuery::BackendNode {
                            id: *tab.snapshot_refs.get(&label).ok_or_else(|| {
                                anyhow::anyhow!("Stale reference; take a new snapshot")
                            })?,
                        },
                        frame_id: None,
                        index: None,
                    },
                    Target::Index(index) => {
                        let element = tab.elements.get(index).ok_or_else(|| {
                            anyhow::anyhow!("Index unavailable; observe the page first")
                        })?;
                        LocatorSpec::css(&element.selector)
                    }
                    Target::Live(_) => commands::live_locator(target)?,
                })
            } else {
                None
            };
            let action = commands::action(cmd, args, target)?;
            Ok((tab.page.clone(), id, action, commands::options(args)?))
        }
        .await;
        match prepared {
            Ok((page, id, action, options)) => {
                commands::response(self.operations.submit(page, id, action, options).await)
            }
            Err(error) => Response::err_detail(
                eoka_protocol::ErrorDetail::new("invalid_action", error.to_string())
                    .retryable(false),
            ),
        }
    }
}
