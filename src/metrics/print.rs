use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

use super::Consumer;

pub struct Print<T: 'static + Write + Send> {
    csv_writer: Arc<Mutex<csv::Writer<T>>>,
}

impl<T: 'static + Write + Send> Print<T> {
    pub fn new(writer: T) -> Self {
        Self {
            csv_writer: Arc::new(Mutex::new(csv::Writer::from_writer(writer))),
        }
    }

    async fn write_record_not_blocking(&self, record: Vec<String>) -> Result<(), String> {
        let csv_writer_clone = Arc::clone(&self.csv_writer);

        tokio::task::spawn_blocking(move || {
            csv_writer_clone
                .lock()
                .map_err(|error| format!("Failed to acquire lock with error: {}", error))
                .and_then(|mut writer| {
                    writer.write_record(&record).map_err(|error| {
                        format!("Failed to write record: {:?} with error: {}", record, error)
                    })
                })
        })
        .await
        .map_err(|error| {
            format!(
                "Failed to execute spawn blocking code with error: {}",
                error
            )
        })?
    }

    async fn flush_not_blocking(&self) -> Result<(), String> {
        let csv_writer_clone = Arc::clone(&self.csv_writer);

        tokio::task::spawn_blocking(move || {
            csv_writer_clone
                .lock()
                .map_err(|error| format!("Failed to acquire lock with error: {}", error))
                .and_then(|mut writer| {
                    writer
                        .flush()
                        .map_err(|error| format!("Failed to flush data with error: {}", error))
                })
        })
        .await
        .map_err(|error| {
            format!(
                "Failed to execute spawn blocking code with error: {}",
                error
            )
        })?
    }
}

#[async_trait]
impl<T: Write + Send> Consumer for Print<T> {
    async fn consume(
        &self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> Result<(), String> {
        self.write_record_not_blocking(
            vec!["#".to_string()]
                .into_iter()
                .chain(column_names.into_iter())
                .collect(),
        )
        .await?;

        let mut row_index: usize = 1;

        while let Some(entry) = rx.recv().await {
            self.write_record_not_blocking(
                vec![row_index.to_string()]
                    .into_iter()
                    .chain(entry.into_iter())
                    .collect(),
            )
            .await?;
            row_index += 1;
        }

        self.flush_not_blocking().await
    }
}

#[cfg(test)]
mod tests {
    use crate::metrics::{Consumer, Print, Producer};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::Sender;

    struct TestProducer {
        column_names: Vec<String>,
        data_to_send: Vec<String>,
    }
    impl TestProducer {
        fn new(column_names: Vec<String>, data_to_send: Vec<String>) -> Self {
            Self {
                column_names,
                data_to_send,
            }
        }
    }

    #[async_trait]
    impl Producer for TestProducer {
        fn column_names(&self) -> Vec<String> {
            self.column_names.clone()
        }

        async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error> {
            tx.send(self.data_to_send.clone()).await.unwrap();
            Ok(())
        }
    }

    #[tokio::test]
    async fn text_with_commas_is_correctly_escaped() {
        let column_with_commas = "column,with,commas";
        let column_without_commas = "column_without_commas";
        let entry_with_commas = "entry,with,commas";
        let entry_without_commas = "entry_without_commas";

        let test_producer = TestProducer::new(
            vec![
                column_with_commas.to_string(),
                column_without_commas.to_string(),
            ],
            vec![
                entry_with_commas.to_string(),
                entry_without_commas.to_string(),
            ],
        );
        let column_names = test_producer.column_names();
        let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);
        tokio::spawn(async move {
            test_producer.producer_task(tx).await.unwrap();
        });

        let print = Print::new(vec![]);
        print.consume(&mut rx, column_names).await.unwrap();
        let buffer = Arc::try_unwrap(print.csv_writer)
            .unwrap()
            .into_inner()
            .unwrap()
            .into_inner()
            .unwrap();
        let output_data = String::from_utf8(buffer).unwrap();

        assert_eq!(
            output_data,
            format!(
                "#,\"{}\",{}\n1,\"{}\",{}\n",
                column_with_commas, column_without_commas, entry_with_commas, entry_without_commas
            )
        )
    }
}
