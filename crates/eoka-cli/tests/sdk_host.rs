use eoka_sdk::{kill_daemon, EokaClient, LaunchSpec, OpenArgs};

#[tokio::test]
#[ignore = "requires Chrome; a program that is not the eoka CLI starts a daemon through the SDK"]
async fn a_program_other_than_the_cli_starts_the_daemon() {
    let scratch = tempfile::tempdir().unwrap();
    std::env::set_var(eoka_protocol::env::BIN, env!("CARGO_BIN_EXE_eoka"));
    std::env::set_var(eoka_protocol::env::STATE_DIR, scratch.path().join("state"));
    let session = format!("sdk-host-{}", uuid::Uuid::new_v4());
    let client = EokaClient::new(
        session.clone(),
        LaunchSpec::Launch {
            headless: true,
            from_profile: None,
            clone_state_from: None,
            no_stealth: false,
            proxy: None,
            no_js: false,
            js_allow: Vec::new(),
            js_block: Vec::new(),
            persist: false,
            geo_align: false,
            auto_solve_cloudflare: false,
            sandbox: true,
        },
    );
    let opened = client
        .open(
            serde_json::from_value::<OpenArgs>(serde_json::json!({
                "url": "data:text/html,<title>hosted</title><p>from another program</p>"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(opened.ok, "{opened:?}");
    let text = client.text().await.unwrap();
    assert!(
        text.data
            .unwrap()
            .to_string()
            .contains("from another program"),
        "the daemon the SDK started answers reads"
    );
    assert!(kill_daemon(&session).unwrap());
}
