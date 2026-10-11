use clap::{Args, Subcommand};
use eoka_protocol::{DownloadIdArgs, DownloadStartArgs, DownloadWaitArgs, Request, Response};
use std::path::PathBuf;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
#[command(group(clap::ArgGroup::new("download_trigger").args(["click", "url"]).required(true).multiple(false)))]
pub struct DownloadCli {
    #[command(subcommand)]
    pub action: Option<DownloadAction>,
    #[arg(
        long,
        help = "Existing click target: @e1, text:Download, or css:button"
    )]
    pub click: Option<String>,
    #[arg(long, help = "Direct HTTP(S) attachment URL")]
    pub url: Option<String>,
    #[arg(
        long = "out",
        required = true,
        help = "New output filename; parent must exist"
    )]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub expect_bytes: Option<u64>,
    #[arg(long, default_value_t = 30_000)]
    pub start_timeout_ms: u64,
    #[arg(long, help = "Wait for this job without blocking other commands")]
    pub wait: bool,
    #[arg(long, default_value_t = 30_000)]
    pub timeout_ms: u64,
}

#[derive(Subcommand)]
pub enum DownloadAction {
    List,
    Status {
        id: String,
    },
    Wait {
        id: String,
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
    },
    Cancel {
        id: String,
    },
}

impl DownloadCli {
    pub fn request(&self) -> Result<Request, String> {
        Ok(match &self.action {
            Some(DownloadAction::List) => Request::DownloadList,
            Some(DownloadAction::Status { id }) => {
                Request::DownloadGet(DownloadIdArgs { id: id.clone() })
            }
            Some(DownloadAction::Wait { id, timeout_ms }) => {
                Request::DownloadWait(DownloadWaitArgs {
                    id: id.clone(),
                    timeout_ms: *timeout_ms,
                })
            }
            Some(DownloadAction::Cancel { id }) => {
                Request::DownloadCancel(DownloadIdArgs { id: id.clone() })
            }
            None => {
                let output = self.output.as_ref().ok_or("--out is required")?;
                let output = if output.is_absolute() {
                    output.clone()
                } else {
                    std::env::current_dir()
                        .map_err(|e| e.to_string())?
                        .join(output)
                };
                let output = output
                    .to_str()
                    .ok_or("Output path must be valid UTF-8")?
                    .to_owned();
                let args = DownloadStartArgs {
                    click: self.click.clone(),
                    url: self.url.clone(),
                    output,
                    expect_bytes: self.expect_bytes,
                    start_timeout_ms: self.start_timeout_ms,
                };
                args.validate()?;
                Request::DownloadStart(args)
            }
        })
    }
}

pub async fn run(
    args: &DownloadCli,
    session: &str,
    spec: crate::launch_spec::LaunchSpec,
    json: bool,
) -> Response {
    let request = match args.request() {
        Ok(request) => request,
        Err(error) => return Response::err(error),
    };
    let response = match crate::client::send_command(session, request, spec.clone()).await {
        Ok(response) => response,
        Err(error) => return Response::err(error.to_string()),
    };
    if !response.ok || args.action.is_some() || !args.wait {
        return response;
    }
    let Some(id) = response.data.as_ref().and_then(|data| data["id"].as_str()) else {
        return response;
    };
    if !json {
        eprintln!("Download {id} started; waiting for the saved file");
    }
    let mut result = match crate::client::send_command(
        session,
        Request::DownloadWait(DownloadWaitArgs {
            id: id.into(),
            timeout_ms: args.timeout_ms,
        }),
        spec,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => Response::err_detail(
            eoka_protocol::ErrorDetail::new("download_transport_error", error.to_string())
                .retryable(false),
        ),
    };
    if let Some(detail) = result.error_detail.as_mut() {
        detail
            .context
            .get_or_insert_with(Default::default)
            .download_id
            .get_or_insert_with(|| id.to_owned());
        detail.hint.get_or_insert_with(|| {
            format!("Inspect download status {id} or wait again; do not replay the start action.")
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn lost_wait_response_keeps_known_id_without_replaying_start() {
        use eoka_protocol::{read_msg, write_msg};
        use tokio::net::UnixListener;
        let session = format!("download-wait-loss-{}", std::process::id());
        crate::session::ensure_runtime_dir().unwrap();
        let path = crate::session::socket_path(&session);
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut start, _) = listener.accept().await.unwrap();
            assert!(matches!(
                read_msg::<_, Request>(&mut start).await.unwrap(),
                Request::DownloadStart(_)
            ));
            write_msg(
                &mut start,
                &Response::ok(serde_json::json!({"id":"known-job","state":"pending"})),
            )
            .await
            .unwrap();
            drop(start);
            let (mut wait, _) = listener.accept().await.unwrap();
            let request: Request = read_msg(&mut wait).await.unwrap();
            assert!(matches!(request, Request::DownloadWait(ref args) if args.id == "known-job"));
            drop(wait);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(150), listener.accept())
                    .await
                    .is_err()
            );
        });
        let cli = crate::cli::Cli::try_parse_from([
            "eoka",
            "download",
            "--click",
            "css:button",
            "--out",
            "unused.bin",
            "--wait",
        ])
        .unwrap();
        let Some(crate::cli::Command::Download(args)) = cli.command else {
            panic!()
        };
        let response = run(
            &args,
            &session,
            crate::launch_spec::LaunchSpec::Connect {
                ws_url: "unused".into(),
            },
            true,
        )
        .await;
        assert!(!response.ok);
        assert!(response.data.is_none());
        let error = response.error_detail.unwrap();
        assert_eq!(error.code, "download_transport_error");
        assert_eq!(
            error.context.unwrap().download_id.as_deref(),
            Some("known-job")
        );
        server.await.unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cli_triggers_are_exclusive_and_paths_are_absolute() {
        for args in [
            vec!["eoka", "download", "--out", "file"],
            vec![
                "eoka",
                "download",
                "--click",
                "@e1",
                "--url",
                "https://example.test/file",
                "--out",
                "file",
            ],
            vec!["eoka", "download", "--click", "@e1"],
        ] {
            assert!(crate::cli::Cli::try_parse_from(args).is_err());
        }
        let cli = crate::cli::Cli::try_parse_from([
            "eoka", "download", "--click", "@e1", "--out", "file", "--wait",
        ])
        .unwrap();
        let Some(crate::cli::Command::Download(args)) = cli.command else {
            panic!()
        };
        assert!(args.wait);
        let Request::DownloadStart(request) = args.request().unwrap() else {
            panic!()
        };
        assert_eq!(
            PathBuf::from(request.output),
            std::env::current_dir().unwrap().join("file")
        );
        for args in [
            vec!["eoka", "download", "list"],
            vec!["eoka", "download", "status", "job"],
            vec!["eoka", "download", "wait", "job"],
            vec!["eoka", "download", "cancel", "job"],
        ] {
            assert!(crate::cli::Cli::try_parse_from(args).is_ok());
        }
    }
}
