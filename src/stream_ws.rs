use super::error::JsonError;

use super::stream::TObjectStream;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Message;

use super::error::Result;

pub struct ObjectStream {
    conn: WebSocketStream<TcpStream>,
}

impl ObjectStream {
    pub async fn new(stream: TcpStream) -> Result<Self> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let obj_stream = Self { conn: ws_stream };
        Ok(obj_stream)
    }
}

#[async_trait]
impl TObjectStream<String> for ObjectStream {
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