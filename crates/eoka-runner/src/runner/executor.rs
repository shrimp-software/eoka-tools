use crate::config::actions::{
    EmailAction, EmailExtractAction, EmailFilterAction, ImapConfigAction, ScrollDirection,
    TryClickAnyAction, WaitForEmailAction,
};
use crate::config::{Action, Config, Params};
use crate::{Error, Result};
use chrono::Duration as ChronoDuration;
use eoka::Page;
use eoka_email::{
    extract_code, extract_first_link, AsyncImapClient, ImapConfig, LinkFilter, SearchCriteria,
    WaitOptions,
};
use regex::Regex;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

impl From<&ImapConfigAction> for ImapConfig {
    fn from(a: &ImapConfigAction) -> Self {
        Self {
            host: a.host.clone(),
            port: a.port,
            tls: a.tls,
            username: a.username.clone(),
            password: a.password.clone(),
            mailbox: a.mailbox.clone(),
        }
    }
}

const MAX_INCLUDE_DEPTH: usize = 10;

#[derive(Clone)]
pub struct ExecutionContext {
    pub operations: eoka_server::operations::OperationManager,
    pub base_path: PathBuf,
    pub include_depth: usize,
}

impl ExecutionContext {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            operations: Default::default(),
            base_path: base_path.into(),
            include_depth: 0,
        }
    }

    pub fn child(&self, new_base: impl Into<PathBuf>) -> Result<Self> {
        if self.include_depth >= MAX_INCLUDE_DEPTH {
            return Err(Error::Config(format!(
                "maximum include depth ({}) exceeded",
                MAX_INCLUDE_DEPTH
            )));
        }
        Ok(Self {
            operations: self.operations.clone(),
            base_path: new_base.into(),
            include_depth: self.include_depth + 1,
        })
    }

    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        }
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_default())
    }
}

pub async fn execute_with_context(
    page: &Page,
    action: &Action,
    ctx: &ExecutionContext,
) -> Result<()> {
    match action {
        Action::Perform(_)
        | Action::Goto(_)
        | Action::Back
        | Action::Forward
        | Action::Reload
        | Action::Click(_)
        | Action::TryClick(_)
        | Action::Fill(_)
        | Action::Type(_)
        | Action::Clear(_)
        | Action::Select(_)
        | Action::PressKey(_)
        | Action::Hover(_)
        | Action::ScrollTo(_) => {
            let prepared = super::operations::prepare(action)?;
            super::operations::run(page, ctx, prepared).await?;
        }
        Action::Wait(a) => {
            debug!("wait: {}ms", a.ms);
            page.wait(a.ms).await;
        }
        Action::WaitForNetworkIdle(a) => {
            debug!(
                "wait_for_network_idle: idle={}ms, timeout={}ms",
                a.idle_ms, a.timeout_ms
            );
            page.wait_for_network_idle(a.idle_ms, a.timeout_ms).await?;
        }
        Action::WaitForText(a) => {
            debug!("wait_for_text: '{}'", a.text);
            page.wait_for_text(&a.text, a.timeout_ms).await?;
        }
        Action::WaitForUrl(a) => {
            debug!("wait_for_url: contains '{}'", a.contains);
            page.wait_for_url_contains(&a.contains, a.timeout_ms)
                .await?;
        }
        Action::WaitForEmail(a) => {
            wait_for_email(page, a).await?;
        }
        Action::TryClickAny(a) => {
            debug!(
                "try_click_any: {:?}",
                a.texts.as_ref().or(a.selectors.as_ref())
            );
            try_click_any(page, a, ctx).await?;
        }
        Action::SetCookie(a) => {
            debug!("set_cookie: {}={}", a.name, a.value);
            page.set_cookie(&a.name, &a.value, a.domain.as_deref(), a.path.as_deref())
                .await?;
        }
        Action::DeleteCookie(a) => {
            debug!("delete_cookie: {}", a.name);
            page.delete_cookie(&a.name, a.domain.as_deref()).await?;
        }
        Action::Execute(a) => {
            debug!("execute: {}...", a.js.chars().take(50).collect::<String>());
            page.execute(&a.js).await?;
        }
        Action::Screenshot(a) => {
            info!("screenshot: {}", a.path);
            let data = page.screenshot().await?;
            tokio::fs::write(&a.path, data).await?;
        }
        Action::Log(a) => {
            info!("[log] {}", a.message);
        }
        Action::AssertText(a) => {
            debug!("assert_text: '{}'", a.text);
            let text = page.text().await?;
            if !text.contains(&a.text) {
                return Err(Error::AssertionFailed(format!(
                    "text '{}' not found",
                    a.text
                )));
            }
        }
        Action::AssertUrl(a) => {
            debug!("assert_url: contains '{}'", a.contains);
            let url = page.url().await?;
            if !url.contains(&a.contains) {
                return Err(Error::AssertionFailed(format!(
                    "url does not contain '{}'",
                    a.contains
                )));
            }
        }
        Action::Scroll(a) => {
            debug!("scroll: {:?} x{}", a.direction, a.amount);
            scroll(page, &a.direction, a.amount).await?;
        }
        Action::WaitFor(a) => {
            debug!("wait_for: {}", a.selector);
            page.wait_for(&a.selector, a.timeout_ms).await?;
        }
        Action::WaitForVisible(a) => {
            debug!("wait_for_visible: {}", a.selector);
            page.wait_for_visible(&a.selector, a.timeout_ms).await?;
        }
        Action::WaitForHidden(a) => {
            debug!("wait_for_hidden: {}", a.selector);
            page.wait_for_hidden(&a.selector, a.timeout_ms).await?;
        }
        Action::IfTextExists(a) => {
            let text = page.text().await?;
            let exists = text.contains(&a.text);
            debug!("if_text_exists '{}': {}", a.text, exists);
            let actions = if exists {
                &a.then_actions
            } else {
                &a.else_actions
            };
            for action in actions {
                Box::pin(execute_with_context(page, action, ctx)).await?;
            }
        }
        Action::IfSelectorExists(a) => {
            let exists = match page.find(&a.selector).await {
                Ok(_) => true,
                Err(eoka::Error::ElementNotFound(_)) => false,
                Err(error) => return Err(error.into()),
            };
            debug!("if_selector_exists '{}': {}", a.selector, exists);
            let actions = if exists {
                &a.then_actions
            } else {
                &a.else_actions
            };
            for action in actions {
                Box::pin(execute_with_context(page, action, ctx)).await?;
            }
        }
        Action::Repeat(a) => {
            debug!("repeat: {} times", a.times);
            for i in 0..a.times {
                debug!("repeat iteration {}/{}", i + 1, a.times);
                for action in &a.actions {
                    Box::pin(execute_with_context(page, action, ctx)).await?;
                }
            }
        }
        Action::Include(a) => {
            let path = ctx.resolve_path(&a.path);
            info!("include: {}", path.display());

            let mut params = Params::new();
            for (k, v) in &a.params {
                params = params.set(k.clone(), v.clone());
            }

            let included_config = Config::load_with_params(&path, &params).map_err(|e| {
                Error::Config(format!(
                    "failed to load include '{}': {}",
                    path.display(),
                    e
                ))
            })?;

            let child_base = path.parent().unwrap_or(Path::new("."));
            let child_ctx = ctx.child(child_base)?;

            for action in &included_config.actions {
                Box::pin(execute_with_context(page, action, &child_ctx)).await?;
            }
        }
    }
    Ok(())
}

async fn wait_for_email(page: &Page, action: &WaitForEmailAction) -> Result<()> {
    let imap = ImapConfig::from(&action.imap);

    let criteria = build_email_criteria(&action.filter);

    let options = WaitOptions::new(
        ChronoDuration::milliseconds(action.timeout_ms as i64),
        ChronoDuration::milliseconds(action.poll_interval_ms as i64),
    );

    let mut client = AsyncImapClient::connect(&imap)
        .await
        .map_err(|e| Error::ActionFailed(e.to_string()))?;

    let msg = client
        .wait_for_message(&criteria, &options)
        .await
        .map_err(|e| Error::ActionFailed(e.to_string()))?;

    let (link, code) = extract_email_values(&msg, &action.extract)?;

    match &action.action {
        Some(EmailAction::OpenLink(_)) => {
            let link =
                link.ok_or_else(|| Error::ActionFailed("no link extracted from email".into()))?;
            info!("email link: {}", link);
            page.goto(&link).await?;
        }
        Some(EmailAction::Fill(fill)) => {
            let code =
                code.ok_or_else(|| Error::ActionFailed("no code extracted from email".into()))?;
            info!("email code: {}", code);
            page.fill(&fill.selector, &code).await?;
        }
        None => {
            if let Some(link) = link {
                info!("email link: {}", link);
            }
            if let Some(code) = code {
                info!("email code: {}", code);
            }
        }
    }

    Ok(())
}

fn build_email_criteria(filter: &EmailFilterAction) -> SearchCriteria {
    let mut criteria = SearchCriteria::new()
        .unseen_only(filter.unseen_only)
        .mark_seen(filter.mark_seen);

    if let Some(ref from) = filter.from {
        criteria = criteria.from(from.clone());
    }
    if let Some(ref subject) = filter.subject_contains {
        criteria = criteria.subject_contains(subject.clone());
    }
    if let Some(minutes) = filter.since_minutes {
        criteria = criteria.since_minutes(minutes);
    }

    criteria
}

fn extract_email_values(
    msg: &eoka_email::EmailMessage,
    extract: &EmailExtractAction,
) -> Result<(Option<String>, Option<String>)> {
    let mut link: Option<String> = None;
    let mut code: Option<String> = None;

    if let Some(ref link_cfg) = extract.link {
        let filter = LinkFilter {
            allow_domains: link_cfg.allow_domains.clone(),
        };
        link = extract_first_link(msg, &filter);
    }

    if let Some(ref code_cfg) = extract.code {
        let re = Regex::new(&code_cfg.regex)
            .map_err(|e| Error::ActionFailed(format!("invalid code regex: {}", e)))?;
        code = extract_code(msg, &re);
    }

    Ok((link, code))
}

async fn scroll(page: &Page, direction: &ScrollDirection, amount: u32) -> Result<()> {
    let distance = i64::from(amount) * 300;
    let (x, y) = match direction {
        ScrollDirection::Up => (0, -distance),
        ScrollDirection::Down => (0, distance),
        ScrollDirection::Left => (-distance, 0),
        ScrollDirection::Right => (distance, 0),
    };
    page.execute(&format!("window.scrollBy({x}, {y})")).await?;
    Ok(())
}

async fn try_click_any(
    page: &Page,
    action: &TryClickAnyAction,
    ctx: &ExecutionContext,
) -> Result<()> {
    let group = match &action.operation.operation_id {
        Some(id) => uuid::Uuid::parse_str(id).map_err(|e| Error::Config(e.to_string()))?,
        None => uuid::Uuid::new_v4(),
    };
    let targets = action
        .selectors
        .iter()
        .flatten()
        .map(eoka::LocatorSpec::css)
        .chain(action.texts.iter().flatten().map(eoka::LocatorSpec::text));
    let started = tokio::time::Instant::now();
    let timeout = action.operation.timeout_ms.unwrap_or(30000);
    for (index, target) in targets.enumerate() {
        let remaining =
            timeout.saturating_sub(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        if remaining == 0 {
            return Err(Error::Config("try_click_any deadline exceeded".into()));
        }
        let id = uuid::Uuid::new_v5(&group, &index.to_be_bytes()).to_string();
        tracing::info!(operation_id=%id,group_id=%group,"Trying durable click candidate");
        let record = ctx
            .operations
            .submit(
                page.clone(),
                id.clone(),
                eoka::Action::Click {
                    target,
                    human: false,
                    double: false,
                },
                eoka::ActionOptions {
                    timeout_ms: remaining,
                    expect: action.operation.expect.clone(),
                    wait_for_target: false,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| Error::Config(e.to_string()))?;
        let record = if matches!(record.state, eoka_protocol::OperationState::Running) {
            ctx.operations
                .wait(&id, remaining.saturating_add(1000))
                .await
                .map_err(|e| Error::Config(e.to_string()))?
        } else {
            record
        };
        if record.outcome == eoka::ActionOutcome::VerifiedComplete {
            return Ok(());
        }
        if record.outcome != eoka::ActionOutcome::NotDispatched
            || record.error_code.as_deref() != Some("target_unavailable")
        {
            return Err(Error::Config(format!(
                "Operation {id}: {:?} ({})",
                record.outcome,
                record.error_code.unwrap_or_default()
            )));
        }
    }
    Ok(())
}
