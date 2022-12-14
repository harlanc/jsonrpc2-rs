use {super::error::Result, async_trait::async_trait};
#[async_trait]
pub trait TObjectStream<T> {
    async fn write_object(&mut self, obj: T) -> Result<()>;
    async fn read_object(&mut self) -> Result<T>;
    async fn close(&mut self) -> Result<()>;
}
