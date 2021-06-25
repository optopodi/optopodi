use async_trait::async_trait;
use tokio::sync::mpsc::{self, Receiver, Sender};

mod export_to_sheets;
mod gql;
mod list_repos;
mod print;
mod repo_participants;
mod util;

#[async_trait]
pub trait Producer {
    fn column_names(&self) -> Vec<String>;
    async fn producer_task(self, tx: Sender<Vec<String>>) -> anyhow::Result<()>;
}

#[async_trait]
pub trait Consumer {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> anyhow::Result<()>;
}

pub use export_to_sheets::ExportToSheets;
pub use gql::Graphql;
pub use list_repos::ListReposForOrg;
pub use print::Print;
pub use repo_participants::RepoParticipants;

/// Spawns a task running a producer and returns the column names
/// that it will produce along with
/// a receiver for the actual columns.
pub fn run_producer(
    producer: impl Producer + Send + 'static,
) -> (Vec<String>, Receiver<Vec<String>>) {
    let (tx, rx) = mpsc::channel::<Vec<String>>(400);
    let column_names = producer.column_names();
    tokio::spawn(async move {
        if let Err(e) = producer.producer_task(tx).await {
            println!("Encountered an error while collecting data: {}", e);
        };
    });

    (column_names, rx)
}
