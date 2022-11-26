use {
    super::{
        error::{JsonError, Result},
        stream::TObjectStream,
    },
    async_trait::async_trait,
    futures_util::{SinkExt, StreamExt},
    tokio::net::TcpStream,
    tokio_tungstenite::{MaybeTlsStream, WebSocketStream},
    tungstenite::protocol::Message,
    url::Url,
};

use std::{net::SocketAddr, time::Duration};

pub struct ServerObjectStream {
    conn: WebSocketStream<TcpStream>,
}

impl ServerObjectStream {
    pub async fn accept(stream: TcpStream) -> Result<Self> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let obj_stream = Self { conn: ws_stream };
        Ok(obj_stream)
    }
}

#[async_trait]
impl TObjectStream<String> for ServerObjectStream {
    async fn read_object(&mut self) -> Result<String> {
        // let (mut ws_sender, mut ws_receiver) = self.conn.split();

        // let mut interval = tokio::time::interval(Duration::from_millis(1000));
        // loop {
        //     tokio::select! {
        //         msg = self.conn.next() => {
        //             // match msg {
        //             //     Some(msg) => {
        //             //         let msg = msg?;
        //             //         if msg.is_text() ||msg.is_binary() {
        //             //             ws_sender.send(msg).await?;
        //             //         } else if msg.is_close() {
        //             //             break;
        //             //         }
        //             //     }
        //             //     None => break,
        //             // }
        //         }
        //         // _ = interval.tick() => {
        //         //     ws_sender.send(Message::Text("tick".to_owned())).await?;
        //         // }
        //     }
        // }
        match self.conn.next().await {
            Some(msg) => {
                let m = msg?;
                if !m.is_text() {
                    return Err(JsonError::ErrWebsocketTypeNotCorrect);
                }
                return Ok(m.into_text()?);
            }
            None => {}
        }
        Err(JsonError::ErrNoneValue)
    }

    async fn write_object(&mut self, obj: String) -> Result<()> {
        let msg = Message::Text(obj);
        self.conn.send(msg).await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.conn.close(None).await?;
        Ok(())
    }
}

pub struct ClientObjectStream {
    conn: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl ClientObjectStream {
    pub async fn connect(url: Url) -> Result<Self> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
        let obj_stream = Self { conn: ws_stream };
        Ok(obj_stream)
    }
}

#[async_trait]
impl TObjectStream<String> for ClientObjectStream {
    async fn read_object(&mut self) -> Result<String> {
        match self.conn.next().await {
            Some(msg) => {
                let m = msg?;
                if !m.is_text() {
                    return Err(JsonError::ErrWebsocketTypeNotCorrect);
                }
                return Ok(m.into_text()?);
            }
            None => {}
        }
        Err(JsonError::ErrNoneValue)
    }

    async fn write_object(&mut self, obj: String) -> Result<()> {
        let msg = Message::Text(obj);
        self.conn.send(msg).await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.conn.close(None).await?;
        Ok(())
    }
}
