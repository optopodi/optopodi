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
        println!(
            "{}\t{}\n-----------------------------------",
            column_names[1], column_names[0]
        );
        while let Some(entry) = rx.recv().await {
            println!("{}\t\t{}", &entry[1], &entry[0]);
        }

        Ok(())
    }
}
