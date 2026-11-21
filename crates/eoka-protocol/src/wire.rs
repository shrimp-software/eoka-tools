use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::sidecar::{
    core_error_code, ErrorCode, ErrorData, ServerError, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use crate::{ErrorDetail, Request, Response};

pub const HELLO: &str = "server.hello";

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &Value,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(frame).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

pub async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> std::io::Result<Option<Value>> {
    let mut line = Vec::new();
    let read = (&mut *reader)
        .take(MAX_MESSAGE_BYTES as u64 + 2)
        .read_until(b'\n', &mut line)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if line.pop() != Some(b'\n') {
        return Err(std::io::Error::other(if line.len() > MAX_MESSAGE_BYTES {
            "message exceeds maxMessageBytes"
        } else {
            "connection closed mid-message"
        }));
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(std::io::Error::other)
}

pub async fn call<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    frame: Option<&Value>,
) -> std::io::Result<Response> {
    let closed = || std::io::Error::other("connection closed");
    write_frame(writer, &hello_frame()).await?;
    let hello = response(read_frame(reader).await?.ok_or_else(closed)?);
    let Some(frame) = frame.filter(|_| hello.ok) else {
        return Ok(hello);
    };
    write_frame(writer, frame).await?;
    Ok(response(read_frame(reader).await?.ok_or_else(closed)?))
}

pub async fn accept<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    hello: &Value,
) -> std::io::Result<Option<(Value, Request)>> {
    let Some(frame) = read_frame(reader).await? else {
        return Ok(None);
    };
    let first = incoming(frame).map_err(|(_, error)| std::io::Error::other(error.message))?;
    write_frame(
        writer,
        &reply_frame(&first.id, &Response::ok(hello.clone())),
    )
    .await?;
    let Some(frame) = read_frame(reader).await? else {
        return Ok(None);
    };
    let next = incoming(frame).map_err(|(_, error)| std::io::Error::other(error.message))?;
    let request = next
        .request()
        .map_err(|error| std::io::Error::other(error.message))?;
    Ok(Some((next.id, request)))
}

pub fn hello_frame() -> Value {
    json!({"id": 1, "method": HELLO, "params": {"protocol": PROTOCOL_VERSION}})
}

pub fn request_frame(cmd: &str, params: Value) -> Value {
    let method = crate::operation_by_cmd(cmd).map_or(cmd, |operation| operation.path);
    json!({"id": 2, "method": method, "params": params})
}

pub fn request(request: &Request) -> Value {
    request_frame(request.cmd(), request.args_json())
}

pub struct Incoming {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

pub fn incoming(frame: Value) -> Result<Incoming, (Value, ServerError)> {
    let id = frame.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = frame.get("method").and_then(Value::as_str) else {
        return Err((
            id,
            ServerError::invalid_params("missing string field `method`"),
        ));
    };
    let params = match frame.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(params) => params.clone(),
    };
    Ok(Incoming {
        method: method.to_owned(),
        id,
        params,
    })
}

impl Incoming {
    pub fn protocol_matches(&self) -> bool {
        self.params["protocol"]["major"] == PROTOCOL_VERSION.major
    }

    pub fn request(&self) -> Result<Request, ServerError> {
        let operation = crate::operation_by_path(&self.method)
            .ok_or_else(|| ServerError::unknown_method(&self.method))?;
        crate::request_from_cmd(operation.cmd, self.params.clone())
            .map_err(ServerError::invalid_params)
    }
}

pub fn error_frame(id: &Value, error: &ServerError) -> Value {
    json!({"id": id, "error": error})
}

pub fn reply_frame(id: &Value, response: &Response) -> Value {
    if response.ok {
        return json!({"id": id, "result": response.data.clone().unwrap_or(Value::Null)});
    }
    let detail = response.error_detail.clone();
    let code = detail
        .as_ref()
        .and_then(|detail| {
            core_error_code(&detail.code).or_else(|| ErrorCode::for_blocker_detail(&detail.code))
        })
        .unwrap_or(ErrorCode::Operation);
    let message = response
        .error
        .clone()
        .or_else(|| detail.as_ref().map(|detail| detail.message.clone()))
        .unwrap_or_else(|| "Command failed".to_owned());
    let error = ServerError::new(code, message).with_data(ErrorData {
        detail,
        result: response.data.clone(),
        ..Default::default()
    });
    error_frame(id, &error)
}

pub fn response(reply: Value) -> Response {
    if let Some(result) = reply.get("result") {
        let mut response = Response::ok(result.clone());
        response.data = response.data.filter(|data| !data.is_null());
        return response;
    }
    let error: Option<ServerError> = reply
        .get("error")
        .and_then(|error| serde_json::from_value(error.clone()).ok());
    let Some(error) = error else {
        return Response::err_detail(ErrorDetail::new(
            "transport_error",
            "Session reply has neither result nor error",
        ));
    };
    let data = error.data.map(|data| *data).unwrap_or_default();
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_else(|| "operation".to_owned());
    let mut response = Response::err_detail(
        data.detail
            .unwrap_or_else(|| ErrorDetail::new(code, error.message.clone())),
    );
    response.error = Some(error.message);
    response.data = data.result;
    response
}
