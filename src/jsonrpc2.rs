use std::fmt::format;

use super::error::JsonError;
use super::error::Result;
use super::stream::ObjectStream;
use async_trait::async_trait;
use serde::ser::{SerializeStruct, Serializer};
use serde::Deserialize;
use serde::Serialize;
use std::result::Result as StdResult;
use std::sync::atomic::AtomicU64;
use tokio::sync::mpsc::UnboundedReceiver;

use beef::Cow;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;
use serde_json::Deserializer;

use std::collections::HashMap;
use std::str;
use std::sync::atomic::Ordering;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
struct Request {
    #[serde(rename = "jsonrpc2")]
    jsonrpc2: String,
    #[serde(rename = "method")]
    method: String,
    #[serde(rename = "params", skip_serializing_if = "Option::is_none")]
    params: Option<String>,
    //if id is none , then it is a notification
    //https://www.jsonrpc.org/specification#notification
    #[serde(rename = "id", skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
}

impl Request {
    pub fn new(method: String, params: Option<String>, id: Option<Id>) -> Self {
        Self {
            jsonrpc2: "2.0".to_string(),
            method,
            params,
            id,
        }
    }

    pub fn marshal_json(&self) -> Result<String> {
        let data = serde_json::to_string(self)?;
        Ok(data)
    }
    pub fn unmarshal_json(&mut self, data: &String) -> Result<()> {
        let request: Request = serde_json::from_str(data)?;
        self.jsonrpc2 = request.jsonrpc2;
        self.method = request.method;
        self.params = request.params;
        self.id = request.id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::Id;

    use std::str::FromStr;

    use super::Request;
    use super::Response;

    use super::AnyMessage;
    //https://juejin.cn/post/6844904105698148360
    #[test]
    fn test_request() {
        let method = String::from_str("add").unwrap();
        let params = String::from_str("[1,2]").unwrap();
        let id = Id::Number(3);

        let request = Request::new(method, Some(params), Some(id));
        let serialized = request.marshal_json().unwrap();

        assert_eq!(
            serialized,
            r#"{"jsonrpc2":"2.0","method":"add","params":"[1,2]","id":3}"#
        );

        let mut default = Request::default();
        default.unmarshal_json(&serialized).unwrap();

        assert_eq!(request, default);
    }
    #[test]
    fn test_any_message() {
        let method = String::from_str("add").unwrap();
        let params = String::from_str("[1,2]").unwrap();
        let id = Id::Number(3);
        let request = Request::new(method, Some(params), Some(id));

        println!("marshal request: {}", request.marshal_json().unwrap());

        let any1 = AnyMessage::Request(request);

        let marshal_msg = serde_json::to_string(&any1).unwrap();
        println!("marshal AnyMessage request: {}", marshal_msg);

        let data: AnyMessage = serde_json::from_str(&marshal_msg).unwrap();

        match data {
            AnyMessage::Request(req) => {
                println!("marshal request 2: {}", req.marshal_json().unwrap());
            }
            AnyMessage::Response(res) => {
                println!("marshal response 2: {}", res.marshal_json().unwrap());
            }
        }

        let id2 = Id::Str(String::from_str("3").unwrap());
        let result = Some(String::from_str("[2,3]").unwrap());
        let response = Response::new(id2, result, None);

        println!("marshal response: {}", response.marshal_json().unwrap());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Response {
    #[serde(rename = "jsonrpc2")]
    jsonrpc2: String,
    #[serde(rename = "id")]
    id: Id,
    #[serde(rename = "result", skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(rename = "error", skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
}

impl Response {
    pub fn new(id: Id, result: Option<String>, error: Option<Error>) -> Self {
        Self {
            jsonrpc2: "2.0".to_string(),
            result,
            error,
            id,
        }
    }
    pub fn marshal_json(&self) -> Result<String> {
        let data = serde_json::to_string(self)?;
        Ok(data)
    }
    pub fn unmarshal_json(&mut self, data: &String) -> Result<()> {
        let response: Response = serde_json::from_str(data)?;
        self.jsonrpc2 = response.jsonrpc2;
        self.result = response.result;
        self.error = response.error;
        self.id = response.id;
        Ok(())
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
//#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[derive(Debug, PartialEq, Clone, Hash, Eq, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
enum Id {
    Number(u64),
    Str(String),
    Null,
}

impl Id {}

pub type ResponseSender = mpsc::UnboundedSender<String>;
pub type ResponseReceiver = mpsc::UnboundedReceiver<String>;

struct Conn {
    stream: Arc<Mutex<dyn ObjectStream<String> + Send + Sync>>,
    handler: Arc<dyn Handler + Send + Sync>,
    shut_down: bool,
    closing: bool,
    seq: AtomicU64,
    response_senders: Arc<Mutex<HashMap<Id, ResponseSender>>>,
}

impl Conn {
    fn new(
        stream: Arc<Mutex<dyn ObjectStream<String> + Send + Sync>>,
        h: Arc<dyn Handler + Send + Sync>,
    ) -> Self {
        Self {
            stream: stream,
            handler: h,
            shut_down: false,
            closing: false,
            seq: AtomicU64::new(0),
            response_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn close(&mut self) -> Result<()> {
        if self.shut_down || self.closing {
            return Err(JsonError::ErrClosed);
        }

        self.closing = true;
        self.stream.lock().await.close().await
    }

    async fn send(&self, msg: AnyMessage) -> Result<()> {
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

        let send_msg = serde_json::to_string(&msg)?;
        self.stream.lock().await.write_object(send_msg).await?;

        Ok(())
    }

    async fn call<T>(
        &self,
        method: String,
        params: Option<String>,
        is_notify: bool,
    ) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let mut id: Option<Id> = None;

        let (sender, mut receiver) = mpsc::unbounded_channel();
        if !is_notify {
            let id_value = Id::Number(self.seq.load(Ordering::Relaxed));
            id = Some(id_value.clone());
            self.seq.fetch_add(1, Ordering::Relaxed);
            self.response_senders.lock().await.insert(id_value, sender);
        }

        let msg = AnyMessage::Request(Request::new(method, params, id));
        self.send(msg).await?;

        if is_notify {
            return Ok(None);
        }

        if let Some(message) = receiver.recv().await {
            return Ok(Some(serde_json::from_str::<T>(&message)?));
        }

        Ok(None)
    }

    async fn read_messages(slf: Arc<Mutex<Self>>) {
        let s = slf.lock().await;
        loop {
            if let Ok(msg) = s.stream.lock().await.read_object().await {
                // let mut any_message = AnyMessage::default();
                // if let Err(err) = any_message.unmarshal_json(msg) {}

                if let Ok(any_message) = serde_json::from_str::<AnyMessage>(&msg) {
                    match any_message {
                        AnyMessage::Request(req) => {
                            s.handler.handle(slf.clone(), req);
                        }
                        AnyMessage::Response(res) => {
                            match s.response_senders.lock().await.get_mut(&res.id) {
                                Some(sender) => {}
                                None => {
                                    log::error!(
                                        "the responsd sender with id: {} is none",
                                        serde_json::to_string(&res.id).unwrap()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
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
    fn handle(&self, conn: Arc<Mutex<Conn>>, request: Request);
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
#[serde(untagged)]
enum AnyMessage {
    Request(Request),
    Response(Response),
}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// struct Msg {
//     #[serde(rename = "id")]
//     id: serde_json::Value,
//     #[serde(rename = "method")]
//     method: String,
//     #[serde(rename = "result")]
//     result: Option<serde_json::Value>,
//     #[serde(rename = "error")]
//     error: Option<serde_json::Value>,
// }

impl AnyMessage {
    // pub fn marshal_json(&self) -> Result<String> {
    //     // if let Some(request) = &self.request {
    //     //     return Ok(serde_json::to_string(request)?);
    //     // } else if let Some(response) = &self.response {
    //     //     return Ok(serde_json::to_string(response)?);
    //     // }

    //     serde_json::to_string(self)?;

    //     Err(JsonError::ErrIllegalMessage)
    // }
    // pub fn unmarshal_json(&mut self, data: String) -> Result<()> {
    //     // let any_msg: Msg = serde_json::from_str(&data)?;

    //     // if any_msg.method.is_empty() {
    //     //     let msg: Request = serde_json::from_str(&data)?;
    //     //     self.request = Some(msg);
    //     //     return Ok(());
    //     // } else if any_msg.result.is_some() || any_msg.error.is_some() {
    //     //     let msg: Response = serde_json::from_str(&data)?;
    //     //     self.response = Some(msg);
    //     //     return Ok(());
    //     // }

    //     Err(JsonError::ErrIllegalMessage)
    // }
}
