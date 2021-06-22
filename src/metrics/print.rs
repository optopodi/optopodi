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
        let csv_writer_clone = self.csv_writer.clone();

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
        let csv_writer_clone = self.csv_writer.clone();

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
        self,
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
