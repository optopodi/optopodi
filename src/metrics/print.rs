use super::Consumer;
use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

pub struct Print;

#[async_trait]
impl Consumer for Print {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> Result<(), String> {
        println!("#,{}", column_names.join(","));
        let mut count = 1;

        while let Some(entry) = rx.recv().await {
            println!("{},{}", count, entry.join(","));
            count += 1;
        }
        Ok(())
    }
}
