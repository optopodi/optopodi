use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

use crate::google_sheets::Sheets;

use super::Consumer;

pub struct ExportToSheets {
    sheet_id: String,
}

impl ExportToSheets {
    pub fn new(sheet_id: &str) -> Self {
        ExportToSheets {
            sheet_id: String::from(sheet_id),
        }
    }
}

#[async_trait]
impl Consumer for ExportToSheets {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> Result<(), anyhow::Error> {
        let sheets = Sheets::initialize(&self.sheet_id).await?;

        // clear existing data from sheet
        sheets
            .clear_sheet()
            .await
            .context("Failed to clear the sheet")?;

        // add headers / column titles
        sheets
            .append(column_names)
            .await
            .context("Failed to append the column names")?;

        // wait for `tx` to send data
        while let Some(entry) = rx.recv().await {
            sheets
                .append(entry)
                .await
                .context(format!("Failed to append entry"))?;
        }

        println!(
            "Successfully uploaded data to Google Sheets: {}",
            sheets.get_link_to_sheet()
        );

        Ok(())
    }
}
