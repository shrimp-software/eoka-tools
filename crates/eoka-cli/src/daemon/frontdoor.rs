use std::time::{Duration, Instant};

use eoka_server::operations::OperationManager;
use serde_json::Value;
use tokio::{
    io::AsyncReadExt,
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixListener,
    },
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::protocol::{read_msg, write_msg, Request, Response};

type AcceptedRequest = (OwnedReadHalf, OwnedWriteHalf, Request, Value);

#[derive(Clone, Copy)]
pub(super) struct Activity {
    pub last_request: Instant,
    pub waiting: bool,
}

pub(super) async fn serve(
    listener: UnixListener,
    hello: Value,
    operations: OperationManager,
    accepted: mpsc::Sender<AcceptedRequest>,
    activity: watch::Sender<Activity>,
) {
    let mut waits = JoinSet::new();
    loop {
        activity.send_replace(Activity {
            last_request: Instant::now(),
            waiting: !waits.is_empty(),
        });
        let connection = tokio::select! {
            connection = listener.accept() => connection,
            _ = waits.join_next(), if !waits.is_empty() => continue,
        };
        let Ok((stream, _)) = connection else { break };
        let (mut reader, mut writer) = stream.into_split();
        let raw: Value =
            match tokio::time::timeout(Duration::from_secs(5), read_msg(&mut reader)).await {
                Ok(Ok(raw)) => raw,
                _ => continue,
            };
        let req: Request = match serde_json::from_value(raw.clone()) {
            Ok(req) => req,
            Err(error) => {
                reply(
                    &mut writer,
                    Response::err(format!("Invalid command request: {error}")),
                )
                .await;
                continue;
            }
        };
        if matches!(req, Request::SessionHello) {
            reply(&mut writer, Response::ok(hello.clone())).await;
            continue;
        }
        if raw["protocol_major"] != eoka_protocol::sidecar::PROTOCOL_VERSION.major {
            reply(&mut writer, Response::err_detail(eoka_protocol::ErrorDetail::new("protocol_mismatch", "Client and daemon protocols differ; upgrade explicitly. No browser was restarted and no command was dispatched."))).await;
            continue;
        }
        let args = raw.get("args").cloned().unwrap_or(Value::Null);
        if matches!(req, Request::OperationWait(_)) {
            if waits.len() >= 64 {
                reply(
                    &mut writer,
                    busy("Too many operation waiters; no action was admitted"),
                )
                .await;
                continue;
            }
            let operations = operations.clone();
            waits.spawn(async move {
                let mut disconnected = [0u8; 1];
                tokio::select! {
                    response = operations.query(req.cmd(), &args) => {
                        if let Some(response) = response { reply(&mut writer, response).await; }
                    }
                    _ = reader.read(&mut disconnected) => {}
                }
            });
            continue;
        }
        if let Some(response) = operations.query(req.cmd(), &args).await {
            reply(&mut writer, response).await;
            continue;
        }
        if let Err(error) = accepted.try_send((reader, writer, req, raw)) {
            let (_, mut writer, _, _) = error.into_inner();
            reply(
                &mut writer,
                busy("Command queue is full; no command was admitted"),
            )
            .await;
        }
    }
}

fn busy(message: &str) -> Response {
    Response::err_detail(eoka_protocol::ErrorDetail::new("daemon_busy", message))
}

async fn reply(writer: &mut OwnedWriteHalf, response: Response) {
    match tokio::time::timeout(Duration::from_secs(5), write_msg(writer, &response)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("[eoka] control response disconnected: {error}"),
        Err(_) => eprintln!("[eoka] control response timed out"),
    }
}
