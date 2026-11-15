use std::path::{Path, PathBuf};

use crate::cli::{InterceptAction, NetworkAction, NetworkRecordAction};
use crate::protocol::{
    ClearFlagArgs, IdArgs, InterceptAddArgs, NetworkExportArgs, NetworkLogArgs,
    NetworkRecordStartArgs, NetworkShowArgs, NetworkWaitArgs, Request,
};

pub(super) fn network_action_to_request(action: &NetworkAction) -> Request {
    match action.clone() {
        NetworkAction::Record { action } => match action {
            NetworkRecordAction::Start {
                patterns,
                no_bodies,
                max_body_bytes,
                clear,
            } => Request::NetworkRecordStart(NetworkRecordStartArgs {
                patterns,
                no_bodies,
                max_body_bytes,
                clear,
            }),
            NetworkRecordAction::Stop => Request::NetworkRecordStop,
            NetworkRecordAction::Status => Request::NetworkRecordStatus,
        },
        NetworkAction::Log {
            limit,
            pattern,
            method,
            status,
            since,
            compact,
        } => Request::NetworkLog(NetworkLogArgs {
            limit,
            pattern,
            method,
            status,
            since,
            compact,
        }),
        NetworkAction::Show { id, body, max_body } => {
            Request::NetworkShow(NetworkShowArgs { id, body, max_body })
        }
        NetworkAction::Har { path, settle_ms } => network_export_request(&path, "har", settle_ms),
        NetworkAction::Export {
            path,
            format,
            settle_ms,
        } => network_export_request(&path, &format, settle_ms),
        NetworkAction::Wait {
            pattern,
            method,
            status,
            timeout,
            since,
            include_existing,
        } => Request::NetworkWait(NetworkWaitArgs {
            pattern,
            method,
            status,
            timeout,
            since,
            include_existing,
        }),
        NetworkAction::Clear => Request::NetworkClear,
        NetworkAction::Intercept { action } => intercept_action_to_request(&action),
    }
}

fn network_export_request(path: &Path, format: &str, settle_ms: Option<u64>) -> Request {
    Request::NetworkExport(network_export_args(path, format, settle_ms))
}

fn network_export_args(path: &Path, format: &str, settle_ms: Option<u64>) -> NetworkExportArgs {
    NetworkExportArgs {
        path: absolute_output_path(path).to_string_lossy().to_string(),
        format: format.to_string(),
        settle_ms,
    }
}

fn intercept_action_to_request(action: &InterceptAction) -> Request {
    match action {
        InterceptAction::Add {
            url_pattern,
            capture,
            respond,
            status,
        } => Request::InterceptAdd(InterceptAddArgs {
            url_pattern: url_pattern.clone(),
            capture: capture
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            respond: respond
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            status: *status,
        }),
        InterceptAction::List => Request::InterceptList,
        InterceptAction::Remove { id } => Request::InterceptRemove(IdArgs { id: id.clone() }),
        InterceptAction::Log { clear } => Request::InterceptLog(ClearFlagArgs { clear: *clear }),
    }
}

fn absolute_output_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}
