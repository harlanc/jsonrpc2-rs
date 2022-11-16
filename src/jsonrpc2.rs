use std::fmt::format;

use super::error::JsonError;
use super::error::Result;
use super::stream::ObjectStream;
use async_trait::async_trait;
use serde::ser::{SerializeStruct, Serializer};
use serde::Deserialize;
use serde::Serialize;
use std::result::Result as StdResult;

use serde_json::Deserializer;

use std::collections::HashMap;
use std::str;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc, oneshot};
// JSONRPC2 describes an interface for issuing requests that speak the
// JSON-RPC 2 protocol.  It isn't really necessary for this package
// itself, but is useful for external users that use the interface as
// an API boundary.
pub trait JsonRpc2 {
    // Call issues a standard request (http://www.jsonrpc.org/specification#request_object).
    fn call<T1, T2>(&mut self, method: String, params: T1, result: T2) -> Result<()>;
    // Notify issues a notification request (http://www.jsonrpc.org/specification#notification).
    fn notify<T>(&mut self, params: T) -> Result<()>;
    // Close closes the underlying connection, if it exists.
    fn close() -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Request {
    #[serde(rename = "jsonrpc2")]
    header: String,
    #[serde(rename = "method")]
    method: String,
    #[serde(rename = "params", skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
    //if id is none , then it is a notification
    //https://www.jsonrpc.org/specification#notification
    #[serde(rename = "id", skip_serializing_if = "Option::is_none")]
    id: Option<ID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Response {
    #[serde(rename = "jsonrpc2")]
    header: String,
    #[serde(rename = "id")]
    id: ID,
    #[serde(rename = "result", skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(rename = "error", skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
}

impl Response {
    fn marshal_json(&self) -> Result<Vec<u8>> {
        if (self.result.is_none() || self.result.clone().unwrap().is_null()) && self.error.is_none()
        {
            return Err(JsonError::ErrCanNotMarshalResponse);
        }
        let val = serde_json::to_vec(self)?;
        Ok(val)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Error {
    #[serde(rename = "code")]
    code: i64,
    #[serde(rename = "message")]
    messge: String,
    #[serde(rename = "data")]
    data: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, Default)]
struct ID {
    // At most one of Num or Str may be nonzero. If both are zero
    // valued, then IsNum specifies which field's value is to be used
    // as the ID.
    num: u64,
    str: String,
    // IsString controls whether the Num or Str field's value should be
    // used as the ID, when both are zero valued. It must always be
    // set to true if the request ID is a string.
    is_string: bool,
}

impl ID {
    fn string(&self) -> String {
        if self.is_string {
            return format!("\"{}\"", self.str);
        }
        self.num.to_string()
    }

    fn marshal_json(&self) -> Vec<u8> {
        if self.is_string {
            return self.str.as_bytes().to_vec();
        }
        self.num.to_be_bytes().to_vec()
    }

    fn unmarshal_json(&self, data: Vec<u8>) {

        //u32::from_be_bytes(data)
    }
}

struct Conn {
    stream: Arc<Mutex<dyn ObjectStream<AnyMessage> + Send + Sync>>,
    handler: Arc<dyn Handler + Send + Sync>,
    shut_down: bool,
    closing: bool,
    seq: u64,
    pending: HashMap<ID, Option<Arc<Call>>>,
}

impl Conn {
    fn new(
        stream: Arc<Mutex<dyn ObjectStream<AnyMessage> + Send + Sync>>,
        h: Arc<dyn Handler + Send + Sync>,
    ) -> Self {
        Self {
            stream: stream,
            handler: h,
            shut_down: false,
            closing: false,
            seq: 0,
            pending: HashMap::new(),
        }
    }

    async fn close(&mut self) -> Result<()> {
        if self.shut_down || self.closing {
            return Err(JsonError::ErrClosed);
        }

        self.closing = true;
        self.stream.lock().await.close().await
    }

    async fn send(&mut self, msg: AnyMessage, wait: bool) -> Result<Option<Arc<Call>>> {
        if self.shut_down || self.closing {
            return Err(JsonError::ErrClosed);
        }

        // let mut id: ID = ID::default();
        // let mut call: Option<Arc<Call>> = None;

        // if msg.request.is_some() && wait {
        //     let mut request = msg.clone().request.unwrap();

        //     let is_id_unset = request.id.str.len() == 0 && request.id.num == 0;
        //     if is_id_unset {
        //         if request.id.is_string {
        //             request.id.str = self.seq.to_string();
        //         } else {
        //             request.id.num = self.seq;
        //         }
        //     }

        //     id = request.id.clone();
        //     call = Some(Arc::new(Call::new(Some(request), None, self.seq)));
        //     self.pending.insert(id.clone(), call.clone());
        //     self.seq += 1;
        // }

        // if let Err(_) = self.stream.lock().await.write_object(msg).await {
        //     self.pending.remove(&id);
        // }

        Ok(None)
    }

    fn call(
        &self,
        method: String,
        params: serde_json::Value,
        result: &serde_json::Value,
    ) -> Result<()> {
        Ok(())
    }

    fn dispatch_call(&self, method: String, params: serde_json::Value) -> Result<()> {
        Ok(())
    }
}

struct Waiter {
    call: Option<Arc<Mutex<Call>>>,
}

impl Waiter {
    async fn wait(&mut self, result: &serde_json::Value) -> Result<()> {
        if let Some(call) = &self.call {
            let ok = call.lock().await.done.recv().await;
        }

        Ok(())
    }
}

trait Handler {
    fn handle(&self, conn: Conn, request: Request);
}

struct Call {
    request: Option<Request>,
    response: Option<Response>,
    seq: u64,
    done: mpsc::UnboundedReceiver<JsonError>,
}

impl Call {
    pub fn new(request: Option<Request>, response: Option<Response>, seq: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            request,
            response,
            seq,
            done: receiver,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnyValueWithExplicitNull {
    null: bool,
    value: Option<serde_json::Value>,
}

impl AnyValueWithExplicitNull {
    fn marshal_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self.value)?)
    }

    fn unmarshal_json(&self, data: Vec<u8>) -> Result<AnyValueWithExplicitNull> {
        let d = trim_ascii_whitespace(&data[..]);

        let vv = str::from_utf8(d).unwrap();
        if vv == "null" {
            return Ok(AnyValueWithExplicitNull {
                null: true,
                value: None,
            });
        }
        let val: AnyValueWithExplicitNull = serde_json::from_slice(&data[..])?;
        Ok(val)
    }
}

pub fn trim_ascii_whitespace(x: &[u8]) -> &[u8] {
    let from = match x.iter().position(|x| !x.is_ascii_whitespace()) {
        Some(i) => i,
        None => return &x[0..0],
    };
    let to = x.iter().rposition(|x| !x.is_ascii_whitespace()).unwrap();
    &x[from..=to]
}

#[derive(Debug, Clone, Default)]
pub struct AnyMessage {
    request: Option<Request>,
    response: Option<Response>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Msg {
    #[serde(rename = "id")]
    id: serde_json::Value,
    #[serde(rename = "method")]
    method: String,
    #[serde(rename = "result")]
    result: Option<serde_json::Value>,
    #[serde(rename = "error")]
    error: Option<serde_json::Value>,
}

impl AnyMessage {
    pub fn unmarshal_json(&mut self, data: String) -> Result<()> {
        let any_msg: Msg = serde_json::from_str(&data)?;

        if any_msg.method.is_empty() {
            let msg: Request = serde_json::from_str(&data)?;
            self.request = Some(msg);
            return Ok(());
        } else if any_msg.result.is_some() || any_msg.error.is_some() {
            let msg: Response = serde_json::from_str(&data)?;
            self.response = Some(msg);
            return Ok(());
        }
        // let mut is_request: bool = false;
        // let mut is_response: bool = false;

        // let mut check_type = |m: Msg| -> Result<()> {
        //     let m_is_request = !m.method.is_empty();
        //     let m_is_response = m.result.null || m.result.value.is_some() || m.error.is_some();

        //     if (!m_is_request && !m_is_response) || (m_is_request && m_is_response) {
        //         return Err(JsonError::ErrUnableDetermineMsgType);
        //     }
        //     if (m_is_request && is_response) || (m_is_response && is_request) {
        //         return Err(JsonError::ErrUnableDetermineMsgType);
        //     }

        //     is_request = m_is_request;
        //     is_response = m_is_response;

        //     Ok(())
        // };

        // let is_array = (data.len() > 0) && (data[0] as char == '[');

        // if is_array {
        //     let msgs: Vec<Msg> = serde_json::from_slice(&data[..])?;
        //     if msgs.len() == 0 {
        //         return Err(JsonError::ErrInvalidEmptyBatch);
        //     }
        //     for msg in msgs {
        //         check_type(msg)?;
        //     }
        // } else {
        //     let msg: Msg = serde_json::from_slice(&data[..])?;
        //     check_type(msg)?;
        // }

        // if is_request && !is_response {
        //     // let request: Request = serde_json::from_slice(&data[..])?;
        //     // self.request = Some(request);
        // } else if !is_request && is_response {
        //     let response: Response = serde_json::from_slice(&data[..])?;
        //     self.response = Some(response);
        // }

        Err(JsonError::ErrIllegalMessage)
    }
}
