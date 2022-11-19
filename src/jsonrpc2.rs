use std::fmt::format;
use std::sync::atomic::AtomicBool;

use super::error::JsonError;
use super::error::Result;
use super::stream::TObjectStream;
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

use super::define::*;

use std::collections::HashMap;
use std::str;
use std::string::String;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc, oneshot};
// JSONRPC2 describes an interface for issuing requests that speak the
// JSON-RPC 2 protocol.  It isn't really necessary for this package
// itself, but is useful for external users that use the interface as
// an API boundary.
#[async_trait]
pub trait JsonRpc2 {
    // Call issues a standard request (http://www.jsonrpc.org/specification#request_object).
    async fn call(&self, method: String, params: Option<String>) -> Result<Option<Response>>;
    // Notify issues a notification request (http://www.jsonrpc.org/specification#notification).
    async fn notify(&self, method: String, params: Option<String>) -> Result<()>;
    // Close closes the underlying connection, if it exists.
    async fn close(&self) -> Result<()>;
    async fn response(&self, response: Response) -> Result<()>;
}

pub trait Handler {
    fn handle(&self, conn: &Conn, request: Request);
}

pub struct Conn {
    stream: Arc<Mutex<dyn TObjectStream<String> + Send + Sync>>,
    handler: Arc<Mutex<dyn Handler + Send + Sync>>,
    closed: AtomicBool,
    seq: AtomicU64,
    response_senders: Arc<Mutex<HashMap<Id, ResponseSender>>>,
}

impl Conn {
    fn new(
        stream: Arc<Mutex<dyn TObjectStream<String> + Send + Sync>>,
        h: Arc<Mutex<dyn Handler + Send + Sync>>,
    ) -> Self {
        let conn = Conn {
            stream: stream,
            handler: h,
            closed: AtomicBool::new(false),
            seq: AtomicU64::new(0),
            response_senders: Arc::new(Mutex::new(HashMap::new())),
        };

        // let arc_conn = Arc::new(Mutex::new(conn));

        // tokio::spawn(async move {
        //     conn.read_messages().await;
        // });

        conn
    }

    async fn send(&self, msg: AnyMessage) -> Result<()> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(JsonError::ErrClosed);
        }

        let send_msg = serde_json::to_string(&msg)?;
        self.stream.lock().await.write_object(send_msg).await?;

        Ok(())
    }
    pub async fn read_messages(&self) {
        loop {
            if let Ok(msg) = self.stream.lock().await.read_object().await {
                if let Ok(any_message) = serde_json::from_str::<AnyMessage>(&msg) {
                    match any_message {
                        AnyMessage::Request(req) => {
                            self.handler.lock().await.handle(self, req);
                        }
                        AnyMessage::Response(res) => {
                            match self.response_senders.lock().await.get_mut(&res.id) {
                                Some(sender) => {
                                    if let Err(err) = sender.send(res) {
                                        log::error!("send response err: {}", err);
                                    }
                                }
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

#[async_trait]
impl JsonRpc2 for Conn {
    async fn close(&self) -> Result<()> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(JsonError::ErrClosed);
        }
        self.closed.store(true, Ordering::Relaxed);
        self.stream.lock().await.close().await
    }

    async fn notify(&self, method: String, params: Option<String>) -> Result<()> {
        let msg = AnyMessage::Request(Request::new(method, params, None));
        self.send(msg).await?;

        Ok(())
    }

    async fn call(&self, method: String, params: Option<String>) -> Result<Option<Response>> {
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let id_num = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = Id::Number(id_num);
        self.response_senders
            .lock()
            .await
            .insert(id.clone(), sender);

        let msg = AnyMessage::Request(Request::new(method, params, Some(id.clone())));
        self.send(msg).await?;

        //wait for the response
        if let Some(response) = receiver.recv().await {
            self.response_senders.lock().await.remove(&id);
            return Ok(Some(response));
        }

        Ok(None)
    }

    async fn response(&self, response: Response) -> Result<()> {
        let msg = AnyMessage::Response(response);
        self.send(msg).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::AnyMessage;
    use super::Id;
    use super::Request;
    use super::Response;
    use std::str::FromStr;

    #[test]
    fn test_request() {
        let method = String::from_str("add").unwrap();
        let params = String::from_str("[1,2]").unwrap();
        let id = Id::Number(3);

        let request = Request::new(method, Some(params), Some(id));
        let serialized = serde_json::to_string(&request).unwrap();

        assert_eq!(
            serialized,
            r#"{"jsonrpc2":"2.0","method":"add","params":"[1,2]","id":3}"#
        );

        let req2 = serde_json::from_str(&serialized).unwrap();
        assert_eq!(request, req2);
    }
    #[test]
    fn test_any_message() {
        let method = String::from_str("add").unwrap();
        let params = String::from_str("[1,2]").unwrap();
        let id = Id::Number(3);
        let request = Request::new(method, Some(params), Some(id));

        println!(
            "marshal request: {}",
            serde_json::to_string(&request).unwrap()
        );

        let any1 = AnyMessage::Request(request);
        let marshal_msg = serde_json::to_string(&any1).unwrap();
        println!("marshal AnyMessage request: {}", marshal_msg);

        let data: AnyMessage = serde_json::from_str(&marshal_msg).unwrap();
        match data {
            AnyMessage::Request(req) => {
                println!(
                    "marshal request 2: {}",
                    serde_json::to_string(&req).unwrap()
                );
            }
            AnyMessage::Response(res) => {
                println!(
                    "marshal response 2: {}",
                    serde_json::to_string(&res).unwrap()
                );
            }
        }

        let id2 = Id::Str(String::from_str("3").unwrap());
        let result = Some(String::from_str("[2,3]").unwrap());
        let response = Response::new(id2, result, None);

        println!(
            "marshal response: {}",
            serde_json::to_string(&response).unwrap()
        );
    }

    use super::Conn;
    use super::Handler;
    use crate::stream_ws::ObjectStream;
    use std::sync::Arc;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::Mutex;

    struct Processor {}

    impl Handler for Processor {
        fn handle(&self, conn: &Conn, request: Request) {
            //conn.
        }
    }

    #[tokio::test]

    async fn test_client_server() {
        let hander = Processor {};

        let addr = "127.0.0.1:9002";
        let listener = TcpListener::bind(&addr).await.expect("Can't listen");
        log::info!("Listening on: {}", addr);

        if let Ok((stream, _)) = listener.accept().await {
            let obj_stream = ObjectStream::new(stream)
                .await
                .expect("cannot generate object stream");

            let conn = Conn::new(
                Arc::new(Mutex::new(obj_stream)),
                Arc::new(Mutex::new(hander)),
            );

            tokio::spawn(async move {
                conn.read_messages().await;
            });

            //let arc_conn = Arc::new(Mutex::new(conn));
        }

        //let conn = Conn::new(stream, h)
    }
}
