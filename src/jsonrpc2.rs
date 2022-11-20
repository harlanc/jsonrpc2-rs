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

#[async_trait]
pub trait JsonRpc2 {
    //http://www.jsonrpc.org/specification#request_object
    async fn call(&self, method: &str, params: Option<&str>) -> Result<Response>;
    //http://www.jsonrpc.org/specification#notification
    async fn notify(&self, method: String, params: Option<String>) -> Result<()>;
    //https://www.jsonrpc.org/specification#response_object
    async fn response(&self, response: Response) -> Result<()>;
    async fn close(&self) -> Result<()>;
}

#[async_trait]
pub trait Handler<T> {
    async fn handle(&self, conn: &Conn, request: Request<T>);
}

pub struct Conn<T> {
    stream: Arc<Mutex<dyn TObjectStream<String> + Send + Sync>>,
    handler: Arc<Mutex<dyn Handler<T> + Send + Sync>>,
    closed: AtomicBool,
    seq: AtomicU64,
    response_senders: Arc<Mutex<HashMap<Id, ResponseSender>>>,
}

impl<T> Conn {
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
                            self.handler.lock().await.handle(self, req).await;
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

    async fn call(&self, method: &str, params: Option<&str>) -> Result<Response> {
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let id_num = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = Id::Number(id_num);
        self.response_senders
            .lock()
            .await
            .insert(id.clone(), sender);

        let params_string = match params {
            Some(s) => Some(String::from(s)),
            None => None,
        };

        let msg = AnyMessage::Request(Request::new(
            String::from(method),
            params_string,
            Some(id.clone()),
        ));
        self.send(msg).await?;

        //wait for the response
        if let Some(response) = receiver.recv().await {
            self.response_senders.lock().await.remove(&id);
            return Ok(response);
        }

        Err(JsonError::ErrNoResponseGenerated)
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
    use super::JsonRpc2;
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
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Test {
        params: Vec<u32>,
    }

    #[test]
    fn test_array() {
        let mut v = Vec::new();
        v.push(1);
        v.push(2);

        let test = Test { params: v };

        let data = serde_json::to_string(&test).unwrap();

        println!("data:{}",data);
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
    use crate::stream_ws::ClientObjectStream;
    use crate::stream_ws::ServerObjectStream;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::signal;
    use tokio::sync::Mutex;
    use tokio::time;

    struct Processor {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct AddParameter {
        params: Vec<u32>,
    }
    #[async_trait]
    impl Handler for Processor {
        async fn handle(&self, conn: &Conn, request: Request) {
            match request.method.as_str() {
                "add" => {
                    println!("params:{}", &request.params.clone().unwrap());
                    let params =
                        serde_json::from_str::<AddParameter>(&request.params.unwrap()).unwrap();
                    let add_res: u32 = params.params.iter().sum();

                    let response =
                        Response::new(request.id.unwrap(), Some(add_res.to_string()), None);
                    conn.response(response).await.unwrap();
                }

                _ => {
                    log::info!("unknow method");
                }
            }
        }
    }

    async fn init_server() {
        let hander = Processor {};

        let addr = "127.0.0.1:9002";
        let listener = TcpListener::bind(&addr).await.expect("Can't listen");
        log::info!("Listening on: {}", addr);
        println!("listening on:{}", addr);

        if let Ok((stream, _)) = listener.accept().await {
            println!("listening on 1:{}", addr);
            let server_stream = ServerObjectStream::accept(stream)
                .await
                .expect("cannot generate object stream");
            println!("listening on 1.1:{}", addr);
            let conn = Conn::new(
                Arc::new(Mutex::new(server_stream)),
                Arc::new(Mutex::new(hander)),
            );
            println!("listening on 2:{}", addr);
            tokio::spawn(async move {
                conn.read_messages().await;
            });
            println!("listening on 3:{}", addr);
        } else {
            println!("listening on 4:{}", addr);
            log::error!("cannot init a tcpstream!!");
        }
        println!("init server finished");
    }

    #[tokio::test]
    async fn test_client_server() {
        tokio::spawn(async move {
            init_server().await;
            signal::ctrl_c().await;
        });

        println!("test_client_server 1");

        time::sleep(time::Duration::new(2, 0)).await;

        let url = url::Url::parse("ws://127.0.0.1:9002/").unwrap();

        let client_stream = ClientObjectStream::connect(url)
            .await
            .expect("cannot generate object stream");

        let hander = Processor {};

        let conn = Conn::new(
            Arc::new(Mutex::new(client_stream)),
            Arc::new(Mutex::new(hander)),
        );

        match conn.call("add", Some("[2,3]")).await {
            Ok(response) => {
                let result = response.result.unwrap();
                print!("result: {}", result);
            }
            Err(err) => {
                log::error!("call add error: {}", err);
            }
        }

        // println!("test_client_server 2");
        // match TcpStream::connect("127.0.0.1:9002").await {
        //     Ok(stream) => {
        //         println!("test_client_server 3");

        //         // let tcp = TcpStream::connect("127.0.0.1:12345")
        //         //     .await
        //         //     .expect("Failed to connect");
        //         let url = url::Url::parse("ws://127.0.0.1:9002/").unwrap();

        //         let client_stream = ClientObjectStream::connect(url)
        //             .await
        //             .expect("cannot generate object stream");

        //         let hander = Processor {};

        //         let conn = Conn::new(
        //             Arc::new(Mutex::new(client_stream)),
        //             Arc::new(Mutex::new(hander)),
        //         );

        //         match conn.call("add", Some("[2,3]")).await {
        //             Ok(response) => {
        //                 let result = response.result.unwrap();
        //                 print!("result: {}", result);
        //             }
        //             Err(err) => {
        //                 log::error!("call add error: {}", err);
        //             }
        //         }
        //     }
        //     Err(err) => {
        //         println!("err:{}", err);
        //         log::error!("cannot connect server!");
        //     }
        // }

        //let conn = Conn::new(stream, h)
    }
}
