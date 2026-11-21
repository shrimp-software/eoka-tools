use super::*;

fn saved_state_with_storage(url: &str) -> SavedState {
    SavedState {
        url: url.into(),
        cookies: Vec::new(),
        local_storage: HashMap::from([("auth".into(), "ok".into())]),
        session_storage: HashMap::from([("flow".into(), "booking".into())]),
        user_agent: String::new(),
        saved_at: String::new(),
    }
}

#[test]
fn state_restore_reload_only_for_web_origins() {
    assert!(state_url_can_reload("https://www.recreation.gov/"));
    assert!(state_url_can_reload("http://127.0.0.1:3000/"));
    assert!(!state_url_can_reload(""));
    assert!(!state_url_can_reload("about:blank"));
    assert!(!state_url_can_reload("data:text/html,hello"));
}

#[test]
fn atomic_open_state_resolves_relative_url_to_saved_origin() {
    let saved = saved_state_with_storage("https://www.recreation.gov/");

    assert_eq!(
        resolve_open_url_for_state("/camping/campsites/71576", Some(&saved)).unwrap(),
        "https://www.recreation.gov/camping/campsites/71576"
    );
    assert_eq!(
        resolve_open_url_for_state("https://other.test/path", Some(&saved)).unwrap(),
        "https://other.test/path"
    );
}

#[test]
fn atomic_open_state_rejects_relative_url_without_saved_origin() {
    let saved = saved_state_with_storage("about:blank");

    assert!(resolve_open_url_for_state("/deep", None).is_err());
    assert!(resolve_open_url_for_state("/deep", Some(&saved)).is_err());
}

#[test]
fn atomic_open_state_primes_only_same_origin_open() {
    let saved = saved_state_with_storage("https://www.recreation.gov/cart");

    assert!(state_should_prime_open(
        &saved,
        "https://www.recreation.gov/camping/campsites/71576"
    ));
    assert!(!state_should_prime_open(
        &saved,
        "https://www.example.com/camping/campsites/71576"
    ));
}

#[test]
fn storage_seed_js_sets_saved_storage_before_navigation_scripts() {
    let saved = saved_state_with_storage("https://www.recreation.gov/");
    let js = build_storage_seed_js(&saved).unwrap();

    assert!(js.contains("location.origin !== \"https://www.recreation.gov\""));
    assert!(js.contains("localStorage.setItem(k,v)"));
    assert!(js.contains("sessionStorage.setItem(k,v)"));
    assert!(js.contains("\"auth\":\"ok\""));
    assert!(js.contains("\"flow\":\"booking\""));
}
