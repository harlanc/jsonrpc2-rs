use {
    serde::{Deserialize, Serialize},
    std::str,
    tokio::sync::mpsc,
};

pub(super) type ResponseNotifier<R, E> = mpsc::UnboundedSender<Response<R, E>>;
pub(super) type AnyMessageSender<S, R, E> = mpsc::UnboundedSender<AnyMessage<S, R, E>>;
pub(super) type AnyMessageReceiver<S, R, E> = mpsc::UnboundedReceiver<AnyMessage<S, R, E>>;

//https://www.jsonrpc.org/specification#request_object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Request<S> {
    #[serde(rename = "jsonrpc2")]
    jsonrpc2: String,
    #[serde(rename = "method")]
    pub method: String,
    //https://www.jsonrpc.org/specification#parameter_structures
    //A Structured value that holds the parameter values to be used during the invocation of the method.
    //This member MAY be omitted.
    #[serde(rename = "params", skip_serializing_if = "Option::is_none")]
    pub params: Option<S>,
    //An identifier established by the Client that MUST contain a String, Number, or NULL value if included.
    //If it is not included it is assumed to be a notification
    //https://www.jsonrpc.org/specification#notification
    #[serde(rename = "id", skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
}
impl<S> Request<S> {
    pub fn new(method: String, params: Option<S>, id: Option<Id>) -> Self {
        Self {
            jsonrpc2: "2.0".to_string(),
            method,
            params,
            id,
        }
    }
}

//https://www.jsonrpc.org/specification#response_object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response<R, E> {
    #[serde(rename = "jsonrpc2")]
    jsonrpc2: String,
    #[serde(rename = "id")]
    pub id: Id,
    //The value of this member is determined by the method invoked on the Server.
    #[serde(rename = "result", skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    #[serde(rename = "error", skip_serializing_if = "Option::is_none")]
    error: Option<Error<E>>,
}
impl<R, E> Response<R, E> {
    pub fn new(id: Id, result: Option<R>, error: Option<Error<E>>) -> Self {
        Self {
            jsonrpc2: "2.0".to_string(),
            result,
            error,
            id,
        }
    }
}
//https://www.jsonrpc.org/specification#error_object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Error<E> {
    #[serde(rename = "code")]
    code: i64,
    #[serde(rename = "message")]
    message: String,
    //A Primitive or Structured value that contains additional information about the error.
    //This may be omitted.
    //The value of this member is defined by the Server
    #[serde(rename = "data", skip_serializing_if = "Option::is_none")]
    data: Option<E>,
}
impl<E> Error<E> {
    pub fn new(code: i64, message: String, data: Option<E>) -> Self {
        Self {
            code,
            message,
            data,
        }
    }
}
//https://www.jsonrpc.org/specification#request_object
//An identifier established by the Client that MUST contain a String, Number, or NULL value if included.
#[derive(Debug, PartialEq, Clone, Hash, Eq, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Id {
    Number(u64),
    Str(String),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AnyMessage<S, R, E> {
    Request(Request<S>),
    Response(Response<R, E>),
    #[serde(skip_serializing)]
    Close(bool),
}
