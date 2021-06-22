use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

use super::Consumer;
use crate::google_sheets::Sheets;

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
    ) -> Result<(), String> {
        let sheets = match Sheets::initialize(&self.sheet_id).await {
            Ok(s) => s,
            Err(e) => return Err(format!("There's been an error! {}", e)),
        };

        // clear existing data from sheet
        if let Err(e) = sheets.clear_sheet().await {
            return Err(format!("There's been an error clearing the sheet: {}", e));
        }

        // add headers / column titles
        if let Err(e) = sheets.append(column_names).await {
            return Err(format!(
                "There's been an error appending the column names {}",
                e
            ));
        }

        // wait for `tx` to send data
        while let Some(data) = rx.recv().await {
            let user_err_message = format!("Had trouble appending repo {}", &data[0]);
            if let Err(e) = sheets.append(data).await {
                return Err(format!("{}: {}", user_err_message, e));
            }
        }

        println!(
            "Successfully uploaded data to Google Sheets: {}",
            sheets.get_link_to_sheet()
        );

        Ok(())
    }
}
