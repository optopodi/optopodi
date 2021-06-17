use anyhow::Error;
use async_trait::async_trait;
use fehler::throws;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::util;

mod export_to_sheets;
mod list_repos;
mod print;

#[async_trait]
pub trait Producer {
    fn column_names(&self) -> Vec<String>;
    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String>;
}

#[async_trait]
pub trait Consumer {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> Result<(), String>;
}

pub use export_to_sheets::ExportToSheets;
pub use list_repos::ListReposForOrg;
pub use print::Print;

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}
