use std::io::{self, Write};

use eoka_protocol::sidecar::{
    ErrorCode, ErrorData, Request, Response, ServerError, MAX_MESSAGE_BYTES,
};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub enum Frame {
    Request(Request),
    Error { id: Value, error: ServerError },
    Empty,
}

pub struct FrameReader<R> {
    input: R,
    bytes: Vec<u8>,
    oversized: bool,
}

impl<R: AsyncBufRead + Unpin> FrameReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            bytes: Vec::new(),
            oversized: false,
        }
    }

    pub async fn next(&mut self) -> io::Result<Option<Frame>> {
        loop {
            let available = self.input.fill_buf().await?;
            if available.is_empty() {
                if self.oversized || !self.bytes.is_empty() {
                    return Ok(Some(self.finish()));
                }
                return Ok(None);
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let length = newline.unwrap_or(available.len());
            if self.bytes.is_empty()
                && !self.oversized
                && newline.is_some()
                && (length <= MAX_MESSAGE_BYTES
                    || (length == MAX_MESSAGE_BYTES + 1 && available[length - 1] == b'\r'))
            {
                let bytes = &available[..length];
                let frame = parse(bytes.strip_suffix(b"\r").unwrap_or(bytes));
                self.input.consume(length + 1);
                return Ok(Some(frame));
            }
            if !self.oversized {
                if length > (MAX_MESSAGE_BYTES + 1).saturating_sub(self.bytes.len()) {
                    self.bytes.clear();
                    self.oversized = true;
                } else {
                    append_bounded(&mut self.bytes, &available[..length], MAX_MESSAGE_BYTES + 1);
                }
            }
            self.input.consume(length + usize::from(newline.is_some()));
            if newline.is_some() {
                return Ok(Some(self.finish()));
            }
        }
    }

    fn finish(&mut self) -> Frame {
        if self.bytes.last() == Some(&b'\r') {
            self.bytes.pop();
        }
        if self.oversized || self.bytes.len() > MAX_MESSAGE_BYTES {
            self.oversized = false;
            self.bytes.clear();
            return Frame::Error {
                id: Value::Null,
                error: ServerError::new(
                    ErrorCode::MessageTooLarge,
                    "request exceeds maxMessageBytes",
                ),
            };
        }
        let frame = parse(&self.bytes);
        self.bytes.clear();
        frame
    }
}

fn append_bounded(buffer: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    if bytes.len().saturating_add(1) > buffer.capacity() - buffer.len() {
        let capacity = buffer
            .capacity()
            .max(8192)
            .saturating_mul(2)
            .max(buffer.len() + bytes.len() + 1)
            .min(limit);
        buffer.reserve_exact(capacity - buffer.len());
    }
    buffer.extend_from_slice(bytes);
}

#[derive(serde::Deserialize)]
struct RecoverId {
    id: u64,
}

fn parse(bytes: &[u8]) -> Frame {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Frame::Empty;
    }
    match serde_json::from_slice::<Request>(bytes) {
        Ok(request) => Frame::Request(request),
        Err(error) => {
            let id = serde_json::from_slice::<RecoverId>(bytes)
                .ok()
                .map(|envelope| envelope.id)
                .filter(|id| (1..=9_007_199_254_740_991).contains(id))
                .map(Value::from)
                .unwrap_or(Value::Null);
            Frame::Error {
                id,
                error: ServerError::invalid_params(error.to_string()),
            }
        }
    }
}

struct LimitedBuffer {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_MESSAGE_BYTES.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("response exceeds maxMessageBytes"));
        }
        append_bounded(&mut self.bytes, bytes, MAX_MESSAGE_BYTES + 1);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn encode(response: &Response, id: &Value, operation_id: Option<&str>) -> io::Result<Vec<u8>> {
    let mut output = LimitedBuffer {
        bytes: Vec::new(),
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut output, response) {
        if !output.exceeded {
            return Err(io::Error::other(error));
        }
        output.bytes.clear();
        output.exceeded = false;
        let error = ServerError::new(
            ErrorCode::MessageTooLarge,
            "response exceeds maxMessageBytes; reduce the response size",
        )
        .with_data(ErrorData {
            operation_id: operation_id.map(str::to_owned),
            ..Default::default()
        });
        serde_json::to_writer(&mut output, &Response::err(id.clone(), error))
            .map_err(io::Error::other)?;
    }
    output.bytes.push(b'\n');
    Ok(output.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn rejects_oversized_input_and_recovers_next_frame() {
        let mut input = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        input.extend_from_slice(b"\n{\"id\":7,\"method\":\"server.hello\"}\n");
        let mut reader = FrameReader::new(BufReader::with_capacity(31, &input[..]));
        assert!(matches!(
            reader.next().await.unwrap(),
            Some(Frame::Error {
                error: ServerError {
                    code: ErrorCode::MessageTooLarge,
                    ..
                },
                ..
            })
        ));
        assert!(
            matches!(reader.next().await.unwrap(), Some(Frame::Request(Request { id, .. })) if id == json!(7))
        );
        assert!(reader.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn handles_split_utf8_and_partial_final_frame() {
        let input = "{\"id\":1,\"method\":\"é\"}\n{\"id\":2,\"method\":\"server.shutdown\"}";
        let mut reader = FrameReader::new(BufReader::with_capacity(1, input.as_bytes()));
        assert!(
            matches!(reader.next().await.unwrap(), Some(Frame::Request(Request { method, .. })) if method == "é")
        );
        assert!(
            matches!(reader.next().await.unwrap(), Some(Frame::Request(Request { id, .. })) if id == json!(2))
        );
        assert!(reader.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn crlf_message_at_ceiling_is_accepted() {
        let mut input = b"{\"id\":1,\"method\":\"server.hello\"}".to_vec();
        input.resize(MAX_MESSAGE_BYTES, b' ');
        input.extend_from_slice(b"\r\n");
        for capacity in [8192, input.len()] {
            let mut reader = FrameReader::new(BufReader::with_capacity(capacity, &input[..]));
            assert!(
                matches!(reader.next().await.unwrap(), Some(Frame::Request(Request { id, .. })) if id == json!(1))
            );
            assert!(reader.bytes.capacity() <= MAX_MESSAGE_BYTES + 1);
        }
    }

    #[test]
    fn output_at_ceiling_has_only_one_extra_terminator_byte() {
        let overhead = serde_json::to_vec(&Response::ok(json!(1), json!("")))
            .unwrap()
            .len();
        let response = Response::ok(
            json!(1),
            Value::String("x".repeat(MAX_MESSAGE_BYTES - overhead)),
        );
        let bytes = encode(&response, &json!(1), None).unwrap();
        assert_eq!(bytes.len(), MAX_MESSAGE_BYTES + 1);
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert!(bytes.capacity() <= MAX_MESSAGE_BYTES + 1);
    }

    #[test]
    fn rejects_oversized_output_without_unbounded_serialized_buffer() {
        let id = json!(9);
        let response = Response::ok(
            id.clone(),
            json!({"result":"x".repeat(MAX_MESSAGE_BYTES + 1)}),
        );
        let bytes = encode(&response, &id, Some("operation")).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["id"], 9);
        assert_eq!(value["error"]["code"], "MessageTooLarge");
        assert_eq!(value["error"]["data"]["operation_id"], "operation");
        assert!(bytes.len() < 1024);
    }
}
