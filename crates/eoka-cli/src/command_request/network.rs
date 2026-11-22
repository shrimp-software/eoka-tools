use std::path::{Path, PathBuf};

use crate::cli::{InterceptAction, NetworkAction, NetworkRecordAction};
use crate::protocol::{NetworkExportArgs, Request};

pub(super) fn network_action_to_request(action: &NetworkAction) -> Request {
    match action.clone() {
        NetworkAction::Record { action } => match action {
            NetworkRecordAction::Start(args) => Request::NetworkRecordStart(args),
            NetworkRecordAction::Stop => Request::NetworkRecordStop,
            NetworkRecordAction::Status => Request::NetworkRecordStatus,
        },
        NetworkAction::Log(args) => Request::NetworkLog(args),
        NetworkAction::Show(args) => Request::NetworkShow(args),
        NetworkAction::Har { path, settle_ms } => network_export_request(NetworkExportArgs {
            path: path.to_string_lossy().to_string(),
            format: "har".to_string(),
            settle_ms,
        }),
        NetworkAction::Export(args) => network_export_request(args),
        NetworkAction::Wait(args) => Request::NetworkWait(args),
        NetworkAction::Clear => Request::NetworkClear,
        NetworkAction::Intercept { action } => match action {
            InterceptAction::Add(args) => Request::InterceptAdd(args),
            InterceptAction::List => Request::InterceptList,
            InterceptAction::Remove(args) => Request::InterceptRemove(args),
            InterceptAction::Log(args) => Request::InterceptLog(args),
        },
    }
}

fn network_export_request(args: NetworkExportArgs) -> Request {
    Request::NetworkExport(NetworkExportArgs {
        path: absolute_output_path(Path::new(&args.path))
            .to_string_lossy()
            .to_string(),
        ..args
    })
}

fn absolute_output_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}
