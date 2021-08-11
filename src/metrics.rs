use async_trait::async_trait;
use stable_eyre::eyre;
use tokio::sync::mpsc::{self, Receiver, Sender};

mod gql;
mod list_repos;
mod print;
mod repo_participants;
mod util;

#[async_trait]
pub trait Producer {
    /// What columns names are produced.
    fn column_names(&self) -> Vec<String>;

    /// Executes the producer and sends columns off to the given "tx" endpoint
    /// of a channel.
    async fn producer_task(self, tx: Sender<Vec<String>>) -> eyre::Result<()>;
}

#[async_trait]
pub trait Consumer {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> eyre::Result<()>;
}

pub use gql::Graphql;
pub use list_repos::ListReposForOrg;
pub use print::Print;
pub use repo_participants::RepoParticipants;
pub use util::all_repos;

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
        }
    });

    (column_names, rx)
}
