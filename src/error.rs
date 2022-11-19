use thiserror::Error as ThisError;
pub type Result<T> = std::result::Result<T, JsonError>;
use serde_json::Error as SerdeJsonError;
use tungstenite::error::Error as WsError;

// #[derive(Debug)]
// pub struct JsonError {
//     pub value: JsonErrorValue,
// }

#[derive(ThisError, Debug)]
pub enum JsonError {
    #[error("none value")]
    ErrNoneValue,

    #[error("jsonrpc2: connection is closed")]
    ErrClosed,

    #[error("jsonrpc2: message must have exactly one of the request or response fields set")]
    ErrAnyMessageFieldsWrong,

    #[error("jsonrpc2: unable to determine message type (request or response)")]
    ErrUnableDetermineMsgType,

    #[error("jsonrpc2: batch message type mismatch (must be all requests or all responses)")]
    ErrMsgTypeMismatch,

    #[error("jsonrpc2: invalid empty batch")]
    ErrInvalidEmptyBatch,

    #[error("jsonrpc2: illegal message")]
    ErrIllegalMessage,

    #[error("websocket error")]
    ErrWebsocket(WsError),

    #[error("serde json error")]
    ErrSerdeJson(SerdeJsonError),

    #[error("the web socket data type is not correct")]
    ErrWebsocketTypeNotCorrect,

    #[error("can't marshal *jsonrpc2.Response (must have result or error)")]
    ErrCanNotMarshalResponse,

    #[error("No response generated after the call function")]
    ErrNoResponseGenerated,
}
// impl Error {
//     pub fn equal(&self, err: &anyhow::Error) -> bool {
//         err.downcast_ref::<Self>().map_or(false, |e| e == self)
//     }
// }

// impl JsonError {
//     pub fn equal(&self, err: &anyhow::Error) -> bool {
//         err.downcast_ref::<Self>().map_or(false, |e| e == self)
//     }
// }

impl From<WsError> for JsonError {
    fn from(error: WsError) -> Self {
        JsonError::ErrWebsocket(error)
    }
}

impl From<SerdeJsonError> for JsonError {
    fn from(error: SerdeJsonError) -> Self {
        JsonError::ErrSerdeJson(error)
    }
}
