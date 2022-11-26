use {
    super::{
        define::*,
        error::{JsonError, Result},
        stream::TObjectStream,
    },
    async_trait::async_trait,
    serde::{de::DeserializeOwned, Serialize},
    std::{
        collections::HashMap,
        str,
        string::String,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc,
        },
    },
    tokio::sync::{mpsc, Mutex},
};

#[async_trait]
pub trait TJsonRpc2<S, R, E> {
    //http://www.jsonrpc.org/specification#request_object C->S
    async fn call(&self, method: &str, params: Option<S>) -> Result<Response<R, E>>;
    //http://www.jsonrpc.org/specification#notification C->S
    fn notify(&self, method: String, params: Option<S>) -> Result<()>;
    //https://www.jsonrpc.org/specification#response_object S->C
    fn response(&self, response: Response<R, E>) -> Result<()>;
}

#[async_trait]
pub trait THandler<S, R, E>
where
    S: Serialize,
{
    async fn handle(&self, conn: Arc<JsonRpc2<S, R, E>>, request: Request<S>);
}

pub struct Conn<S, R, E> {
    stream: Box<dyn TObjectStream<String> + Send + Sync>,
    //handler is used for receiving the Request message
    //and implementing the customized logic
    // handler: Option<Box<dyn THandler<S, R, E> + Send + Sync>>,
    closed: AtomicBool,
    any_message_sender: AnyMessageSender<S, R, E>,
}

impl<S, R, E> Conn<S, R, E>
where
    S: Serialize + DeserializeOwned,
    R: Serialize + DeserializeOwned,
    E: Serialize + DeserializeOwned,
{
    #[allow(dead_code)]

    fn new(
        stream: Box<dyn TObjectStream<String> + Send + Sync>,
        //h: Option<Box<dyn THandler<S, R, E> + Send + Sync>>,
        any_message_sender: AnyMessageSender<S, R, E>,
    ) -> Self
    where
        S: Send + Sync + 'static,
        R: Send + Sync + 'static,
        E: Send + Sync + 'static,
    {
        Self {
            stream: stream,
            //handler: h,
            closed: AtomicBool::new(false),
            any_message_sender,
        }
    }

    async fn send(&mut self, msg: AnyMessage<S, R, E>) -> Result<()> {
        println!("send 1");
        if self.closed.load(Ordering::Relaxed) {
            return Err(JsonError::ErrClosed);
        }
        println!("send 2");
        let send_msg = serde_json::to_string(&msg)?;
        self.stream.write_object(send_msg).await?;
        println!("send 3");
        Ok(())
    }
    pub async fn run_loop(
        &mut self,
        mut receiver_from_jsonrpc2: AnyMessageReceiver<S, R, E>,
        mut sender_to_jsonrpc2: AnyMessageSender<S, R, E>,
        response_notifiers: Arc<Mutex<HashMap<Id, ResponseNotifier<R, E>>>>,
    ) {
        loop {
            tokio::select! {
                msg = self.stream.read_object() => {
                    match msg{
                        Ok(data) =>{
                            if let Ok(any_message) = serde_json::from_str::<AnyMessage<S, R, E>>(&data)
                            {
                                // match any_message {
                                //     AnyMessage::Request(req) => {
                                //         if let Some(handler) = self.handler.take() {
                                //             handler.handle(self, req).await;
                                //         }
                                //     }
                                //     AnyMessage::Response(res) => {
                                //         match response_notifiers.lock().await.get_mut(&res.id) {
                                //             Some(sender) => {
                                //                 if let Err(err) = sender.send(res) {
                                //                     log::error!("send response err: {}", err);
                                //                 }
                                //             }
                                //             None => {
                                //                 log::error!(
                                //                     "the responsd sender with id: {} is none",
                                //                     serde_json::to_string(&res.id).unwrap()
                                //                 );
                                //             }
                                //         }
                                //     }
                                // }

                                sender_to_jsonrpc2.send(any_message);
                            }
                        }
                        Err(err) =>{
                            continue;
                        }
                    }
                }
                any_message = receiver_from_jsonrpc2.recv() =>{
                    println!("get message=======0");
                    if let Some(any_message_data) = any_message{
                        println!("get message=======1");
                        self.send(any_message_data).await;
                        println!("get message=======2");
                    }
                }
            }

            // _ = interval.tick() => {
            //     ws_sender.send(Message::Text("tick".to_owned())).await?;
            // }
        }
    }
    #[allow(dead_code)]
    async fn close(&mut self) -> Result<()> {
        println!("close=======0");
        if self.closed.load(Ordering::Relaxed) {
            return Err(JsonError::ErrClosed);
        }
        println!("close=======1");
        self.closed.store(true, Ordering::Relaxed);
        println!("close=======2");
        self.stream.close().await
    }

    fn response(&self, response: Response<R, E>) -> Result<()> {
        let msg = AnyMessage::Response(response);

        if let Err(_) = self.any_message_sender.send(msg) {
            return Err(JsonError::ErrChannelSendError);
        }
        Ok(())
    }
}

pub struct JsonRpc2<S, R, E> {
    seq: AtomicU64,
    response_notifiers: Arc<Mutex<HashMap<Id, ResponseNotifier<R, E>>>>,
    //handler: Option<Box<dyn THandler<S, R, E> + Send + Sync>>,
    any_msg_sender_to_conn: AnyMessageSender<S, R, E>,
    //any_msg_receiver_from_conn: AnyMessageReceiver<S, R, E>,
}

async fn json_rpc2_run_loop<S, R, E>(
    mut any_msg_receiver_from_conn: AnyMessageReceiver<S, R, E>,
    mut handler: Option<Box<dyn THandler<S, R, E> + Send + Sync>>,
    response_notifiers: Arc<Mutex<HashMap<Id, ResponseNotifier<R, E>>>>,
    conn: Arc<JsonRpc2<S, R, E>>,
) where
    S: Serialize + DeserializeOwned + Sync + Send,
    R: Serialize + DeserializeOwned + Sync + Send,
    E: Serialize + DeserializeOwned + Sync + Send,
{
    loop {
        let json_rpc2 = conn.clone();
        tokio::select! {
            any_message = any_msg_receiver_from_conn.recv() => {
                if let Some(any_message_data) = any_message {
                    match any_message_data {
                        AnyMessage::Request(req) => {
                            if let Some(handler) = handler.take() {
                                 handler.handle(json_rpc2, req).await;
                                    }
                                }
                                AnyMessage::Response(res) => {
                                    match response_notifiers.lock().await.get_mut(&res.id) {
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

impl<S, R, E> JsonRpc2<S, R, E>
where
    S: Serialize + DeserializeOwned,
    R: Serialize + DeserializeOwned,
    E: Serialize + DeserializeOwned,
{
    async fn new(
        stream: Box<dyn TObjectStream<String> + Send + Sync>,
        h: Option<Box<dyn THandler<S, R, E> + Send + Sync>>,
    ) -> Arc<Self>
    where
        S: Send + Sync + 'static,
        R: Send + Sync + 'static,
        E: Send + Sync + 'static,
    {
        let (sender_to_conn, receiver_from_jsonrpc2) = mpsc::unbounded_channel();
        let (sender_to_jsonrpc2, receiver_from_conn) = mpsc::unbounded_channel();

        let mut conn = Conn::new(stream, sender_to_conn.clone());

        let response_notifiers = Arc::new(Mutex::new(HashMap::new()));
        let response_notifiers_clone = response_notifiers.clone();
        let response_notifiers_clone_2 = response_notifiers.clone();

        tokio::spawn(async move {
            conn.run_loop(
                receiver_from_jsonrpc2,
                sender_to_jsonrpc2,
                response_notifiers,
            )
            .await;
        });

        let json_rpc2 = Arc::new(JsonRpc2 {
            seq: AtomicU64::new(0),
            response_notifiers: response_notifiers_clone,
            //handler: h,
            any_msg_sender_to_conn: sender_to_conn,
            //any_msg_receiver_from_conn: receiver_from_conn,
        });

        let json_rpc2_clone = json_rpc2.clone();

        tokio::spawn(async move {
            json_rpc2_run_loop(
                receiver_from_conn,
                h,
                response_notifiers_clone_2,
                json_rpc2_clone,
            )
            .await;
        });

        json_rpc2
    }

    // async fn run_loop(&mut self) {
    //     loop {
    //         tokio::select! {
    //             any_message = self.any_msg_receiver_from_conn.recv() => {
    //                 if let Some(any_message_data) = any_message {
    //                     match any_message_data {
    //                         AnyMessage::Request(req) => {
    //                             if let Some(handler) = self.handler.take() {
    //                                  handler.handle(self, req).await;
    //                                     }
    //                                 }
    //                                 AnyMessage::Response(res) => {
    //                                     match self.response_notifiers.lock().await.get_mut(&res.id) {
    //                                         Some(sender) => {
    //                                             if let Err(err) = sender.send(res) {
    //                                                 log::error!("send response err: {}", err);
    //                                             }
    //                                         }
    //                                         None => {
    //                                             log::error!(
    //                                                 "the responsd sender with id: {} is none",
    //                                                 serde_json::to_string(&res.id).unwrap()
    //                                             );
    //                                         }
    //                                     }
    //                                 }
    //                             }

    //                 }
    //             }

    //         }
    //     }
    // }
}

#[async_trait]
impl<S, R, E> TJsonRpc2<S, R, E> for JsonRpc2<S, R, E>
where
    S: Serialize + DeserializeOwned + Sync + Send,
    R: Serialize + DeserializeOwned + Sync + Send,
    E: Serialize + DeserializeOwned + Sync + Send,
{
    fn notify(&self, method: String, params: Option<S>) -> Result<()> {
        let msg = AnyMessage::Request(Request::new(method, params, None));
        if let Err(_) = self.any_msg_sender_to_conn.send(msg) {
            return Err(JsonError::ErrChannelSendError);
        }
        Ok(())
    }

    async fn call(&self, method: &str, params: Option<S>) -> Result<Response<R, E>> {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        println!("call 0");
        let id_num = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = Id::Number(id_num);
        self.response_notifiers
            .lock()
            .await
            .insert(id.clone(), sender);
        println!("call 1");
        let msg = AnyMessage::Request(Request::new(String::from(method), params, Some(id.clone())));
        if let Err(_) = self.any_msg_sender_to_conn.send(msg) {
            return Err(JsonError::ErrChannelSendError);
        }
        println!("call 2");
        //wait for the response
        if let Some(response) = receiver.recv().await {
            self.response_notifiers.lock().await.remove(&id);
            return Ok(response);
        }
        println!("call 3");
        Err(JsonError::ErrNoResponseGenerated)
    }

    fn response(&self, response: Response<R, E>) -> Result<()> {
        let msg = AnyMessage::Response(response);

        if let Err(_) = self.any_msg_sender_to_conn.send(msg) {
            return Err(JsonError::ErrChannelSendError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::stream_ws::ClientObjectStream;
    use crate::stream_ws::ServerObjectStream;
    use async_trait::async_trait;
    use std::str::FromStr;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;
    use tokio::time;

    #[test]
    fn test_request() {
        let method = String::from_str("add").unwrap();
        let params = vec![1u32, 2u32];
        let id = Id::Number(3);
        let request = Request::new(method, Some(params), Some(id));

        let serialized = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serialized,
            r#"{"jsonrpc2":"2.0","method":"add","params":[1,2],"id":3}"#
        );

        let req2 = serde_json::from_str::<Request<Vec<u32>>>(&serialized).unwrap();
        assert_eq!(request, req2);
    }

    #[test]
    fn test_any_message_request() {
        let method = String::from_str("add").unwrap();
        let params = vec![1_u32, 2_u32];
        let id = Id::Number(3);

        let request = Request::new(method, Some(params), Some(id));
        let request_any = AnyMessage::<_, String, String>::Request(request);

        let marshal_msg = serde_json::to_string(&request_any).unwrap();
        println!("marshal AnyMessage request: {}", marshal_msg);

        let data: AnyMessage<Vec<u32>, String, String> =
            serde_json::from_str(&marshal_msg).unwrap();

        assert_eq!(request_any, data);
    }

    #[test]
    fn test_any_message_response() {
        let id2 = Id::Str(String::from_str("3").unwrap());
        let result = Some(5_u32);
        let response = Response::<_, String>::new(id2, result, None);
        let response_any = AnyMessage::<String, _, _>::Response(response);

        let response_any_str = serde_json::to_string(&response_any).unwrap();
        println!("marshal response: {}", response_any_str);
        let data_response: AnyMessage<String, u32, String> =
            serde_json::from_str(&response_any_str).unwrap();

        assert_eq!(response_any, data_response);
    }

    struct Add {}

    #[async_trait]
    impl THandler<Vec<u32>, u32, String> for Add {
        async fn handle(
            &self,
            conn: Arc<JsonRpc2<Vec<u32>, u32, String>>,
            request: Request<Vec<u32>>,
        ) {
            match request.method.as_str() {
                "add" => {
                    let params = request.params.unwrap();
                    let add_res: u32 = params.iter().sum();
                    let response = Response::new(request.id.unwrap(), Some(add_res), None);
                    conn.response(response).unwrap();
                }

                _ => {
                    log::info!("unknow method");
                }
            }
        }
    }

    async fn init_server() {
        let addr = "127.0.0.1:9002";
        let listener = TcpListener::bind(&addr).await.expect("Can't listen");

        if let Ok((stream, _)) = listener.accept().await {
            let server_stream = ServerObjectStream::accept(stream)
                .await
                .expect("cannot generate object stream");

            let mut server = JsonRpc2::new(Box::new(server_stream), Some(Box::new(Add {}))).await;
        }
        println!("init server finished");
    }

    #[tokio::test]
    async fn test_client_server() {
        tokio::spawn(async move {
            init_server().await;
        });

        time::sleep(time::Duration::new(2, 0)).await;

        let url = url::Url::parse("ws://127.0.0.1:9002/").unwrap();
        let client_stream = ClientObjectStream::connect(url)
            .await
            .expect("cannot generate object stream");

        let conn_arc = JsonRpc2::<_, u32, String>::new(Box::new(client_stream), None).await;

        time::sleep(time::Duration::new(2, 0)).await;
        println!("adfadfadf");
        match conn_arc.call("add", Some(vec![2u32, 3u32, 4u32])).await {
            Ok(response) => {
                let result = response.result.unwrap();
                print!("result: {}", result);
                assert_eq!(result, 9);
                println!("adfadfadf===");
            }
            Err(err) => {
                log::error!("call add error: {}", err);
                println!("adfadfadf+++");
            }
        }

        println!("===============");

        // conn_arc.close().await.unwrap();
    }
}
