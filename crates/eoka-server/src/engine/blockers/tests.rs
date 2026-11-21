use super::*;

fn event(sessions: &mut Sessions, method: &str, params: Value) -> bool {
    sessions.apply(Some("session"), method, &params, Instant::now())
}

#[test]
fn a_dialog_blocks_until_it_closes() {
    let mut sessions = Sessions::default();
    assert!(sessions.current("session", "top").is_none());
    assert!(event(
        &mut sessions,
        "Page.javascriptDialogOpening",
        json!({"type":"confirm","message":"Delete?","url":"https://example.test/"}),
    ));
    let Some(Blocker::Dialog(dialog)) = sessions.current("session", "top") else {
        panic!("dialog expected")
    };
    assert_eq!(dialog.kind, "confirm");
    assert_eq!(dialog.message, "Delete?");
    assert!(event(
        &mut sessions,
        "Page.javascriptDialogClosed",
        json!({"result":true})
    ));
    assert!(sessions.current("session", "top").is_none());
    assert!(!event(
        &mut sessions,
        "Page.javascriptDialogClosed",
        json!({"result":true})
    ));
}

#[test]
fn a_navigation_is_pending_from_start_until_commit_or_stop() {
    let mut sessions = Sessions::default();
    event(
        &mut sessions,
        "Page.frameStartedLoading",
        json!({"frameId":"top"}),
    );
    event(
        &mut sessions,
        "Page.frameStartedNavigating",
        json!({"frameId":"top","url":"https://slow.test/","navigationType":"differentDocument"}),
    );
    let Some(Blocker::Navigation(navigation)) = sessions.current("session", "top") else {
        panic!("navigation expected")
    };
    assert_eq!(navigation.url.as_deref(), Some("https://slow.test/"));
    assert!(event(
        &mut sessions,
        "Page.frameNavigated",
        json!({"frame":{"id":"top","url":"https://slow.test/"}}),
    ));
    assert!(sessions.current("session", "top").is_none());
    assert!(sessions
        .stopped("session", "top", Duration::from_secs(5))
        .is_none());
    event(
        &mut sessions,
        "Page.frameStartedLoading",
        json!({"frameId":"top"}),
    );
    assert!(sessions.current("session", "top").is_some());
    assert!(event(
        &mut sessions,
        "Page.frameStoppedLoading",
        json!({"frameId":"top"})
    ));
    assert!(sessions.current("session", "top").is_none());
    assert!(sessions
        .stopped("session", "top", Duration::from_secs(5))
        .is_some());
    assert!(sessions
        .stopped("session", "child", Duration::from_secs(5))
        .is_none());
    assert!(!event(
        &mut sessions,
        "Page.frameStoppedLoading",
        json!({"frameId":"top"})
    ));
}

#[test]
fn subframes_other_sessions_and_same_document_changes_do_not_block_the_tab() {
    let mut sessions = Sessions::default();
    event(
        &mut sessions,
        "Page.frameStartedLoading",
        json!({"frameId":"child"}),
    );
    assert!(sessions.current("session", "top").is_none());
    assert!(!event(
        &mut sessions,
        "Page.frameStartedNavigating",
        json!({"frameId":"top","url":"https://app.test/#tab","navigationType":"sameDocument"}),
    ));
    assert!(sessions.current("session", "top").is_none());
    sessions.apply(
        Some("other"),
        "Page.javascriptDialogOpening",
        &json!({"type":"alert","message":"x"}),
        Instant::now(),
    );
    assert!(sessions.current("session", "top").is_none());
    assert!(sessions.current("other", "top").is_some());
}

#[test]
fn a_dialog_outranks_a_pending_navigation_and_detach_forgets_the_session() {
    let mut sessions = Sessions::default();
    event(
        &mut sessions,
        "Page.frameStartedNavigating",
        json!({"frameId":"top","url":"https://slow.test/","navigationType":"differentDocument"}),
    );
    event(
        &mut sessions,
        "Page.javascriptDialogOpening",
        json!({"type":"beforeunload","message":""}),
    );
    assert!(matches!(
        sessions.current("session", "top"),
        Some(Blocker::Dialog(_))
    ));
    assert!(sessions.apply(
        None,
        "Target.detachedFromTarget",
        &json!({"sessionId":"session"}),
        Instant::now(),
    ));
    assert!(sessions.current("session", "top").is_none());
}

#[test]
fn errors_name_the_blocker_and_the_next_command() {
    let dialog = Blocker::Dialog(Dialog {
        kind: "alert".into(),
        message: "Saved".into(),
        url: String::new(),
        since: Instant::now(),
    });
    let cli = Remedies::for_surface(Surface::Cli, Some("work"));
    let detail = dialog.detail(&cli, "view", Stage::NotStarted);
    assert_eq!(detail.code, "dialog_open");
    assert!(detail.message.contains("\"Saved\""));
    assert!(detail.message.contains("`view` was not started"));
    let hint = detail.hint.unwrap();
    assert!(hint.contains("`eoka --session work dialog accept`"));
    assert!(hint.contains("`eoka --session work dialog dismiss`"));
    assert_eq!(
        detail.context.unwrap().action_status,
        Some(ActionStatus::NotAttempted)
    );
    let navigation = Blocker::Navigation(Navigation {
        url: Some("https://slow.test/".into()),
        since: Instant::now(),
    });
    let mcp = Remedies::for_surface(Surface::Mcp, None);
    let detail = navigation.detail(&mcp, "page_text", Stage::ReadInterrupted);
    assert!(detail.context.is_none());
    assert!(dialog
        .detail(&mcp, "exec", Stage::ActionInterrupted)
        .hint
        .unwrap()
        .contains("Do not repeat"));
    assert_eq!(detail.code, "navigation_pending");
    assert!(detail.message.contains("https://slow.test/"));
    assert!(detail.message.contains("returned without a result"));
    assert!(detail.hint.unwrap().contains("`reload`"));
    let default = Remedies::for_surface(Surface::Cli, Some("default"));
    assert!(Stop::Unresponsive(READ_LIMIT)
        .detail(&default, "info", Stage::ReadInterrupted)
        .hint
        .unwrap()
        .contains("`eoka tab new URL`"));
}

#[test]
fn commands_are_classified_by_what_they_need_from_the_page() {
    let none = json!({});
    assert_eq!(policy("dialog_accept", &none), Policy::Free);
    assert_eq!(policy("tab_list", &none), Policy::Free);
    assert_eq!(policy("close", &none), Policy::Free);
    assert_eq!(policy("info", &none), Policy::Read);
    assert_eq!(policy("eval", &none), Policy::Script);
    assert_eq!(policy("open", &none), Policy::Navigate);
    assert_eq!(policy("click", &none), Policy::Gate);
    assert_eq!(policy("agent_observe", &none), Policy::Observe);
    assert_eq!(Policy::Observe.interrupted(), Stage::ReadInterrupted);
    assert_eq!(Policy::Script.interrupted(), Stage::ActionInterrupted);
    assert_eq!(policy("screenshot", &none), Policy::Capture);
    assert_eq!(
        policy("screenshot", &json!({"annotate":true})),
        Policy::Gate
    );
    assert_eq!(policy("wait", &json!({"ms":500})), Policy::Free);
    assert_eq!(policy("wait", &json!({"text":"Done"})), Policy::Wait);
    assert_eq!(
        policy("action_command", &json!({"command":"reload"})),
        Policy::Navigate
    );
    assert_eq!(
        policy("action_command", &json!({"command":"click"})),
        Policy::Gate
    );
    assert_eq!(
        policy("agent_step", &json!({"action":{"kind":"navigate"}})),
        Policy::Navigate
    );
    assert_eq!(
        policy("agent_step", &json!({"action":{"kind":"click"}})),
        Policy::Gate
    );
    assert_eq!(limit(Policy::Read, &none), Some(READ_LIMIT));
    assert_eq!(
        limit(Policy::Wait, &json!({"timeout":4000})),
        Some(Duration::from_secs(6))
    );
    assert_eq!(limit(Policy::Script, &none), None);
}

#[test]
fn commands_are_named_the_way_each_surface_spells_them() {
    let cli = Remedies::for_surface(Surface::Cli, None);
    let none = Value::Null;
    assert_eq!(cli.name("agent_observe", &none), "view");
    assert_eq!(cli.name("captcha_datadome", &none), "captcha datadome");
    assert_eq!(cli.name("get_text", &none), "get-text");
    assert_eq!(cli.name("tab_list", &none), "tab list");
    assert_eq!(cli.name("dblclick", &none), "dblclick");
    assert_eq!(cli.name("mouse_down", &none), "mouse-down");
    assert_eq!(cli.name("intercept_add", &none), "network intercept add");
    assert_eq!(cli.name("recording_get", &none), "recording status");
    assert_eq!(
        cli.name("action_command", &json!({"command":"reload"})),
        "reload"
    );
    let catalog = Remedies::for_surface(Surface::Catalog, None);
    assert_eq!(catalog.name("agent_observe", &none), "agent.observe");
    assert_eq!(catalog.new_tab(), "`tab.new`");
    assert_eq!(catalog.status(), "`tab.list`");
    let mcp = Remedies::for_surface(Surface::Mcp, None);
    assert_eq!(mcp.name("page_text", &none), "page_text");
}

#[test]
fn a_missing_expectation_says_how_to_supply_one() {
    let mut response = Response::err_detail(ErrorDetail::new(
        "expectation_required",
        "expectation_required: action requires an expected result",
    ));
    explain(
        &mut response,
        &Remedies::for_surface(Surface::Cli, Some("work")),
    );
    let hint = response.error_detail.unwrap().hint.unwrap();
    assert!(hint.contains("--expect"));
    assert!(hint.contains("explore"));
}
