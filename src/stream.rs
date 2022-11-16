use super::error::Result;

use async_trait::async_trait;

#[async_trait]
pub trait ObjectStream<T> {
    async fn write_object(&mut self, obj: T) -> Result<()>;
    async fn read_object(&mut self) -> Result<T>;
    async fn close(&mut self) -> Result<()>;
}
