use async_trait::async_trait;
use fehler::throws;
use log::debug;
use stable_eyre::eyre::{self, Error};
use tokio::sync::mpsc::Sender;
use toml::value::Datetime;

use super::{util, Graphql, Producer};

#[derive(Debug)]
pub struct ListReposForOrg {
    graphql: Graphql,
    org_name: String,
    repo_names: Vec<String>,
    start_date: Datetime,
    end_date: Datetime,
}

impl ListReposForOrg {
    pub fn new(
        graphql: Graphql,
        org_name: String,
        repo_names: Vec<String>,
        start_date: Datetime,
        end_date: Datetime,
    ) -> Self {
        ListReposForOrg {
            graphql,
            org_name,
            repo_names,
            start_date,
            end_date,
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![
            String::from("Organization"),
            String::from("Repository"),
            String::from("PRs Opened"),
            String::from("Issues Opened"),
            String::from("Issues Closed"),
            String::from("Start Date"),
            String::from("End Date"),
        ]
    }

    async fn producer_task(mut self, tx: Sender<Vec<String>>) -> Result<(), eyre::Error> {
        for repo in &self.repo_names {
            let count_prs = util::count_pull_requests(
                &mut self.graphql,
                &self.org_name,
                &repo,
                &self.start_date,
                &self.end_date,
            )
            .await?;

            let count_issues = count_issue_closures(
                &mut self.graphql,
                &self.org_name,
                &repo,
                &self.start_date,
                &self.end_date,
            )
            .await?;

            tx.send(vec![
                self.org_name.clone(),
                repo.to_owned(),
                count_prs.to_string(),
                count_issues.opened.to_string(),
                count_issues.closed.to_string(),
                self.start_date.to_string(),
                self.end_date.to_string(),
            ])
            .await?;
        }

        Ok(())
    }
}

#[derive(Default, Debug)]
struct IssueClosuresCount {
    opened: usize,
    closed: usize,
}

/// count the number of issues opened and closed in a given time period
///
/// # Arguments
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` Github Organization
/// - `start_date` — The beginning of the relevant time period to search within
/// - `end_date` — The end of the relevant time period to search within
#[throws]
async fn count_issue_closures(
    graphql: &mut Graphql,
    org_name: &str,
    repo_name: &str,
    start_date: &Datetime,
    end_date: &Datetime,
) -> IssueClosuresCount {
    let opened = util::count_issues(
        graphql, org_name, repo_name, start_date, end_date, "created",
    )
    .await?;

    let closed =
        util::count_issues(graphql, org_name, repo_name, start_date, end_date, "closed").await?;

    let result = IssueClosuresCount { opened, closed };
    debug!(
        "Retried issue closure info for {}/{}: {:?}",
        org_name, repo_name, result
    );

    result
}
