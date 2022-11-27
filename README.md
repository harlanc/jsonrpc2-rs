# jsonrpc2-rs


[![crates.io](https://img.shields.io/crates/v/jrpc2.svg)](https://crates.io/crates/jrpc2)

A JSON-RPC 2.0 client/server library in rust.


# How to use

## Define the structed value types

You should define the types of the follwing three structed values:

 - Request Parameters [Reference](https://www.jsonrpc.org/specification#parameter_structures)
 - Response Result [Reference](https://www.jsonrpc.org/specification#response_object)
 - Error Data [Reference](https://www.jsonrpc.org/specification#error_object)

For example:
        
    type RequestParams = Vec<u32>;
    type ResponseResult = u32;
    type ErrorData = String;
       
## Define the handler

   You should implement the THandler trait to define the server side logic:
   
    #[async_trait]
    pub trait THandler<S, R, E>
    where
        S: Serialize,
    {
        async fn handle(&self, conn: Arc<JsonRpc2<S, R, E>>, request: Request<S>);
    } 
    
 For example:
 
    struct Add {}

    #[async_trait]
    impl THandler<RequestParams, ResponseResult, ErrorData> for Add {
        async fn handle(
            &self,
            json_rpc2: Arc<JsonRpc2<RequestParams, ResponseResult, ErrorData>>,
            request: Request<RequestParams>,
        ) {
            match request.method.as_str() {
                "add" => {
                    let params = request.params.unwrap();
                    let add_res: u32 = params.iter().sum();
                    let response = Response::new(request.id.unwrap(), Some(add_res), None);
                    json_rpc2.response(response).unwrap();
                }

                _ => {
                    log::info!("unknow method");
                }
            }
        }
    }

## Init JSON-RPC server

    let addr = "127.0.0.1:9002";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");

    if let Ok((stream, _)) = listener.accept().await {
        let server_stream = ServerObjectStream::accept(stream)
            .await
            .expect("cannot generate object stream");
        JsonRpc2::new(Box::new(server_stream), Some(Box::new(Add {}))).await;
    }
    
## Init JSON-RPC client

    let url = url::Url::parse("ws://127.0.0.1:9002/").unwrap();
    let client_stream = ClientObjectStream::connect(url)
        .await
        .expect("cannot generate object stream");

    let conn_arc =
        JsonRpc2::<_, ResponseResult, ErrorData>::new(Box::new(client_stream), None).await;
    
## Call the function

    match conn_arc.call("add", Some(vec![2u32, 3u32, 4u32])).await {
        Ok(response) => {
            let result = response.result.unwrap();
            assert_eq!(result, 9);
        }
        Err(err) => {
            log::error!("call add error: {}", err);
        }
    }
 