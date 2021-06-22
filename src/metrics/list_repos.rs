use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use tokio::sync::mpsc::Sender;

use super::{query_search, Producer, QuerySearch, GQL};

pub struct ListReposForOrg {
    org_name: String,
    number_of_days: i64,
}

impl ListReposForOrg {
    pub fn new(org_name: String, number_of_days: i64) -> Self {
        ListReposForOrg {
            org_name,
            number_of_days,
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![String::from("Repository Name"), String::from("# of PRs")]
    }

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error> {
        let repos: Vec<String> = super::all_repos_graphql(&self.org_name).await?;

        for repo in &repos {
            let count_prs = count_pull_requests_graphql(
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

/// count the number of pull requests created in the given time period for the given repository within the given GitHub organization
///
/// # Arguments
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` Github Organization
/// - `time_period` — The relevant time period to search within
#[throws]
async fn count_pull_requests_graphql(
    org_name: &str,
    repo_name: &str,
    time_period: Duration,
) -> usize {
    // get date string to match GitHub's PR query format for `created` field
    // i.e., "2021-05-18UTC" turns into "2021-05-18"
    let date_str = chrono::NaiveDate::parse_from_str(
        &format!("{}", (Utc::now() - time_period).date())[..],
        "%FUTC",
    )
    .unwrap();

    let query_string = format!(
        r#"repo:{org_name}/{repo_name} is:pr created:>{date_str}"#,
        org_name = org_name,
        repo_name = repo_name,
        date_str = date_str,
    );

    let response = QuerySearch::execute(query_search::Variables { query_string }).await?;
    let response_data = response.data.expect("missing response data");
    let count = response_data.search.issue_count;
    count as usize
}
