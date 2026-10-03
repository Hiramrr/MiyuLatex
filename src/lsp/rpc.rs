//! JSON-RPC de LSP: mensajes con cabecera `Content-Length` sobre un flujo de bytes.

use serde_json::{Value, json};

/// Más que esto no es un mensaje de un servidor de lenguaje, sino basura.
const MAX_BODY: usize = 64 * 1024 * 1024;
const MAX_HEADER: usize = 8 * 1024;

pub fn encode(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

pub fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

pub fn reply(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn reply_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    /// El flujo ya no es de fiar: hay que abandonar al servidor.
    Fatal(String),
    /// Un mensaje con el cuerpo mal formado; el siguiente se puede leer.
    Skipped(String),
}

/// Junta bytes que llegan en trozos arbitrarios y entrega mensajes completos.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next(&mut self) -> Result<Option<Value>, DecodeError> {
        let Some(end) = self.buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
            if self.buffer.len() > MAX_HEADER {
                return Err(DecodeError::Fatal("cabecera demasiado larga".into()));
            }
            return Ok(None);
        };
        let header = String::from_utf8_lossy(&self.buffer[..end]).into_owned();
        let length = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
            })
            .flatten()
            .ok_or_else(|| {
                let shown: String = header.chars().take(80).collect();
                DecodeError::Fatal(format!("cabecera sin Content-Length válido: {shown:?}"))
            })?;
        if length > MAX_BODY {
            return Err(DecodeError::Fatal(format!("mensaje de {length} bytes")));
        }
        let start = end + 4;
        if self.buffer.len() < start + length {
            return Ok(None);
        }
        let body: Vec<u8> = self.buffer.drain(..start + length).skip(start).collect();
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(|e| DecodeError::Skipped(e.to_string()))
    }
}

pub enum Message {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: i64,
        result: Result<Value, String>,
    },
}

pub fn classify(mut value: Value) -> Option<Message> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let id = value.get("id").filter(|id| !id.is_null()).cloned();
    match (method, id) {
        (Some(method), Some(id)) => Some(Message::Request {
            id,
            method,
            params: value["params"].take(),
        }),
        (Some(method), None) => Some(Message::Notification {
            method,
            params: value["params"].take(),
        }),
        (None, Some(id)) => {
            let result = match value.get("error") {
                Some(error) => Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error del servidor")
                    .to_owned()),
                None => Ok(value["result"].take()),
            };
            Some(Message::Response {
                id: id.as_i64()?,
                result,
            })
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trip_with_multibyte_body() {
        let message = notification("x", json!({"texto": "acción 😀"}));
        let mut decoder = Decoder::default();
        decoder.push(&encode(&message));
        assert_eq!(decoder.next().unwrap(), Some(message));
        assert_eq!(decoder.next().unwrap(), None);
    }

    #[test]
    fn split_messages_are_reassembled_byte_by_byte() {
        let first = request(1, "a", json!({"n": "é😀"}));
        let second = reply(json!(2), json!(null));
        let mut bytes = encode(&first);
        bytes.extend(encode(&second));
        let mut decoder = Decoder::default();
        let mut got = Vec::new();
        for byte in bytes {
            decoder.push(&[byte]);
            while let Some(message) = decoder.next().unwrap() {
                got.push(message);
            }
        }
        assert_eq!(got, vec![first, second]);
    }

    #[test]
    fn several_messages_in_one_chunk() {
        let messages: Vec<Value> = (0..3).map(|i| request(i, "m", json!([i]))).collect();
        let mut decoder = Decoder::default();
        decoder.push(&messages.iter().flat_map(encode).collect::<Vec<_>>());
        for message in &messages {
            assert_eq!(decoder.next().unwrap().as_ref(), Some(message));
        }
        assert_eq!(decoder.next().unwrap(), None);
    }

    #[test]
    fn extra_headers_and_case_are_accepted() {
        let mut decoder = Decoder::default();
        decoder.push(b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}");
        assert_eq!(decoder.next().unwrap(), Some(json!({})));
    }

    #[test]
    fn garbage_is_fatal_and_bad_json_is_skipped() {
        let mut decoder = Decoder::default();
        decoder.push(b"Esto no es LSP\r\n\r\n");
        assert!(matches!(decoder.next(), Err(DecodeError::Fatal(_))));

        let mut decoder = Decoder::default();
        decoder.push(b"Content-Length: 3\r\n\r\n{x}");
        decoder.push(&encode(&json!({"ok": true})));
        assert!(matches!(decoder.next(), Err(DecodeError::Skipped(_))));
        assert_eq!(decoder.next().unwrap(), Some(json!({"ok": true})));

        let mut decoder = Decoder::default();
        decoder.push(&vec![b'a'; MAX_HEADER + 1]);
        assert!(matches!(decoder.next(), Err(DecodeError::Fatal(_))));
        let mut decoder = Decoder::default();
        decoder.push(b"Content-Length: 99999999999\r\n\r\n");
        assert!(matches!(decoder.next(), Err(DecodeError::Fatal(_))));
    }

    #[test]
    fn classify_tells_the_three_kinds_apart() {
        assert!(matches!(
            classify(request(3, "m", json!({}))),
            Some(Message::Request { .. })
        ));
        assert!(matches!(
            classify(notification("n", json!({}))),
            Some(Message::Notification { .. })
        ));
        assert!(matches!(
            classify(reply(json!(4), json!(1))),
            Some(Message::Response {
                id: 4,
                result: Ok(_)
            })
        ));
        assert!(matches!(
            classify(reply_error(json!(5), -1, "no")),
            Some(Message::Response {
                id: 5,
                result: Err(_)
            })
        ));
        assert!(classify(json!({"jsonrpc": "2.0"})).is_none());
    }
}
