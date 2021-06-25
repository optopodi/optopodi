use async_trait::async_trait;
use chrono::Duration;
use tokio::sync::mpsc::Sender;

use super::{util, Graphql, Producer};

#[derive(Debug)]
pub struct ListReposForOrg {
    graphql: Graphql,
    org_name: String,
    repo_names: Vec<String>,
    number_of_days: i64,
}

impl ListReposForOrg {
    pub fn new(
        graphql: Graphql,
        org_name: String,
        repo_names: Vec<String>,
        number_of_days: i64,
    ) -> Self {
        ListReposForOrg {
            graphql,
            org_name,
            repo_names,
            number_of_days,
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![String::from("Repository Name"), String::from("# of PRs")]
    }

    async fn producer_task(mut self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error> {
        for repo in &self.repo_names {
            let count_prs = util::count_pull_requests(
                &mut self.graphql,
                &self.org_name,
                &repo,
                Duration::days(self.number_of_days),
            )
            .await?;
            tx.send(vec![repo.to_owned(), count_prs.to_string()])
                .await?;
        }

        Ok(())
    }
}
