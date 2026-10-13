use super::executor::ExecutionContext;
use crate::{
    config::{actions::Target, Action},
    Error, Result,
};
use eoka::{Action as Core, ActionOptions, LocatorSpec, Page};
use eoka_protocol::{ActionOutcome, ActionPerformArgs, ActionSettings, OperationState};

fn locator(target: &Target) -> Result<LocatorSpec> {
    match (&target.selector, &target.text) {
        (Some(selector), None) => Ok(LocatorSpec::css(selector)),
        (None, Some(text)) => Ok(LocatorSpec::text(text)),
        _ => Err(Error::Config(
            "Exactly one selector or text target is required".into(),
        )),
    }
}

pub(super) fn prepare(action: &Action) -> Result<Option<(ActionPerformArgs, bool)>> {
    let defaults = ActionSettings::default();
    let (action, settings, optional) = match action {
        Action::Perform(request) => return Ok(Some((request.clone(), false))),
        Action::Goto(a) => (
            Core::Navigate {
                url: a.url.clone(),
                referrer: None,
            },
            &a.operation,
            false,
        ),
        Action::Back => (Core::Back, &defaults, false),
        Action::Forward => (Core::Forward, &defaults, false),
        Action::Reload => (Core::Reload, &defaults, false),
        Action::Click(a) => (
            Core::Click {
                target: locator(&a.target)?,
                human: a.human,
                double: false,
            },
            &a.operation,
            false,
        ),
        Action::TryClick(a) => (
            Core::Click {
                target: locator(&a.target)?,
                human: false,
                double: false,
            },
            &a.operation,
            true,
        ),
        Action::Fill(a) => (
            Core::Fill {
                target: locator(&a.target)?,
                value: a.value.clone(),
                human: a.human,
            },
            &a.operation,
            false,
        ),
        Action::Type(a) => (
            Core::Type {
                target: locator(&a.target)?,
                text: a.value.clone(),
                human: false,
            },
            &a.operation,
            false,
        ),
        Action::Clear(a) => (
            Core::Fill {
                target: locator(&a.target)?,
                value: String::new(),
                human: false,
            },
            &a.operation,
            false,
        ),
        Action::Select(a) => (
            Core::SelectOption {
                target: locator(&a.target)?,
                value: a.value.clone(),
            },
            &a.operation,
            false,
        ),
        Action::PressKey(a) => (Core::PressKey { key: a.key.clone() }, &a.operation, false),
        Action::Hover(a) => (
            Core::Hover {
                target: locator(&a.target)?,
                human: false,
            },
            &a.operation,
            false,
        ),
        Action::ScrollTo(a) => (
            Core::ScrollIntoView {
                target: locator(&a.target)?,
            },
            &a.operation,
            false,
        ),
        _ => return Ok(None),
    };
    let options = ActionOptions {
        timeout_ms: settings.timeout_ms.unwrap_or(30000),
        expect: settings.expect.clone(),
        wait_for_target: !optional,
        ..Default::default()
    };
    Ok(Some((
        ActionPerformArgs {
            operation_id: settings
                .operation_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            action,
            options,
        },
        optional,
    )))
}

pub(super) async fn run(
    page: &Page,
    ctx: &ExecutionContext,
    (request, optional): (ActionPerformArgs, bool),
) -> Result<()> {
    let id = request.operation_id.clone();
    tracing::info!(operation_id=%id,"Executing durable action");
    let record = ctx
        .operations
        .submit(page.clone(), id.clone(), request.action, request.options)
        .await
        .map_err(|e| Error::Config(e.to_string()))?;
    let record = if matches!(record.state, OperationState::Running) {
        ctx.operations
            .wait(&id, 86_400_000)
            .await
            .map_err(|e| Error::Config(e.to_string()))?
    } else {
        record
    };
    if record.outcome == ActionOutcome::VerifiedComplete
        || (optional
            && record.outcome == ActionOutcome::NotDispatched
            && record.error_code.as_deref() == Some("target_unavailable"))
    {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "Operation {id}: {:?} ({})",
            record.outcome,
            record.error_code.unwrap_or_default()
        )))
    }
}
