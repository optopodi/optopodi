use async_trait::async_trait;
use fehler::throws;
use log::debug;
use stable_eyre::eyre;
use stable_eyre::eyre::Error;
use tokio::sync::mpsc::Sender;
use toml::value::Datetime;

use super::{Graphql, Producer};

/// Find the number of issue openings and closures in a set of repos in a given time period.
pub struct IssueClosures {
    graphql: Graphql,
    org_name: String,
    repo_names: Vec<String>,
    start_date: Datetime,
    end_date: Datetime,
}

impl IssueClosures {
    pub fn new(
        graphql: Graphql,
        org_name: String,
        repo_names: Vec<String>,
        start_date: Datetime,
        end_date: Datetime,
    ) -> Self {
        Self {
            graphql,
            org_name,
            repo_names,
            start_date,
            end_date,
        }
    }
}

#[async_trait]
impl Producer for IssueClosures {
    fn column_names(&self) -> Vec<String> {
        vec![
            String::from("Organization"),
            String::from("Repository"),
            String::from("Issues Opened"),
            String::from("Issues Closed"),
            String::from("Start Date"),
            String::from("End Date"),
        ]
    }

    async fn producer_task(mut self, tx: Sender<Vec<String>>) -> Result<(), eyre::Error> {
        for repo_name in &self.repo_names {
            let count = count_issue_closures(
                &mut self.graphql,
                &self.org_name,
                repo_name,
                &self.start_date,
                &self.end_date,
            )
            .await?;

            tx.send(vec![
                self.org_name.clone(),
                repo_name.clone(),
                count.opened.to_string(),
                count.closed.to_string(),
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
    async fn count(
        graphql: &mut Graphql,
        org_name: &str,
        repo_name: &str,
        start_date: &Datetime,
        end_date: &Datetime,
        state: &str,
    ) -> Result<usize, eyre::Error> {
        debug!("Fetching issue closure info for {}/{}", org_name, repo_name);
        let response = graphql
            .query(super::util::CountIssues)
            .execute(super::util::count_issues::Variables {
                query_string: format!(
                    r#"repo:{org_name}/{repo_name} is:issue {state}:{start_date}..{end_date}"#,
                    org_name = org_name,
                    repo_name = repo_name,
                    start_date = start_date,
                    end_date = end_date,
                    state = state,
                ),
            })
            .await?;

        let response_data = response.data.expect("missing response data");
        Ok(response_data.search.issue_count as usize)
    }

    let opened = count(
        graphql, org_name, repo_name, start_date, end_date, "created",
    )
    .await?;
    let closed = count(graphql, org_name, repo_name, start_date, end_date, "closed").await?;
    let result = IssueClosuresCount { opened, closed };
    debug!(
        "Retried issue closure info for {}/{}: {:?}",
        org_name, repo_name, result
    );

    result
}
