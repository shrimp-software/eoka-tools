use std::cell::RefCell;

thread_local! {
    static DECODED: RefCell<Vec<(String, Result<(), String>)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn record(cmd: &str, response: &eoka_protocol::Response) {
    let Some(data) = response.data.as_ref() else {
        return;
    };
    let record = data["id"].is_string() && data.get("state").is_some();
    if response.ok || record {
        let outcome = eoka_protocol::decode_result_for_cmd(cmd, data);
        DECODED.with(|decoded| decoded.borrow_mut().push((cmd.to_owned(), outcome)));
    }
}

pub(super) fn take() -> Vec<(String, Result<(), String>)> {
    DECODED.with(|decoded| std::mem::take(&mut *decoded.borrow_mut()))
}
