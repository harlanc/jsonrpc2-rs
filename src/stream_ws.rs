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
