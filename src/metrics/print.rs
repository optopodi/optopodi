use std::io::Write;

use async_trait::async_trait;
use stable_eyre::eyre;
use stable_eyre::eyre::WrapErr;

use tokio::sync::mpsc::Receiver;

use super::Consumer;

pub struct Print<T: 'static + Write + Send> {
    csv_writer: csv::Writer<T>,
}

impl<T: 'static + Write + Send> Print<T> {
    pub fn new(writer: T) -> Self {
        Self {
            csv_writer: csv::Writer::from_writer(writer),
        }
    }
}

#[async_trait]
impl<T: Write + Send> Consumer for Print<T> {
    async fn consume(
        mut self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> eyre::Result<()> {
        self.csv_writer = write_record_not_blocking(
            self.csv_writer,
            vec!["#".to_string()]
                .into_iter()
                .chain(column_names.into_iter())
                .collect(),
        )
        .await
        .wrap_err("Failed to output columns names")?;

        let mut row_index: usize = 1;

        while let Some(entry) = rx.recv().await {
            self.csv_writer = write_record_not_blocking(
                self.csv_writer,
                vec![row_index.to_string()]
                    .into_iter()
                    .chain(entry.into_iter())
                    .collect(),
            )
            .await
            .wrap_err(format!("Failed to output {}-th entry", row_index))?;
            row_index += 1;
        }

        self.csv_writer = flush_not_blocking(self.csv_writer).await?;

        Ok(())
    }
}

async fn write_record_not_blocking<T>(
    mut csv_writer: csv::Writer<T>,
    record: Vec<String>,
) -> eyre::Result<csv::Writer<T>>
where
    T: 'static + Write + Send,
{
    tokio::task::spawn_blocking(move || {
        csv_writer.write_record(&record)?;
        Ok(csv_writer)
    })
    .await?
}

async fn flush_not_blocking<T>(mut csv_writer: csv::Writer<T>) -> eyre::Result<csv::Writer<T>>
where
    T: 'static + Write + Send,
{
    tokio::task::spawn_blocking(move || {
        csv_writer.flush()?;
        Ok(csv_writer)
    })
    .await?
}
