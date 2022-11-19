use serde::{Deserialize, Serialize};
use std::str;
use tokio::sync::mpsc;

pub(super) type ResponseSender = mpsc::UnboundedSender<Response>;
pub(super) type ResponseReceiver = mpsc::UnboundedReceiver<Response>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Request {
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    #[serde(rename = "jsonrpc2")]
    jsonrpc2: String,
    #[serde(rename = "id")]
    pub id: Id,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    #[serde(rename = "code")]
    code: i64,
    #[serde(rename = "message")]
    messge: String,
    #[serde(rename = "data")]
    data: serde_json::Value,
}

#[derive(Debug, PartialEq, Clone, Hash, Eq, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Id {
    Number(u64),
    Str(String),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum AnyMessage {
    Request(Request),
    Response(Response),
}
