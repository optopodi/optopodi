use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use super::{Producer, Response};

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

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String> {
        let repos: Vec<String> = match super::all_repos_graphql(&self.org_name).await {
            Ok(r) => r,
            Err(e) => {
                return Err(format!(
                    "Ran into an error while gathering repositories! {}",
                    e
                ));
            }
        };

        for repo in &repos {
            match count_pull_requests_graphql(
                &self.org_name,
                &repo,
                Duration::days(self.number_of_days),
            )
            .await
            {
                Ok(count_prs) => {
                    if let Err(e) = tx.send(vec![repo.to_owned(), count_prs.to_string()]).await {
                        return Err(format!("{:#?}", e));
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "Ran into an issue while counting PRs for repository {}: {}",
                        &repo, e
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct Data {
    search: IssueCount,
}

#[derive(Deserialize, Debug)]
struct IssueCount {
    #[serde(rename = "issueCount")]
    issue_count: u32,
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

    // the reason you'll see `last:1` in the query below is b/c we don't actually need to iterate through every single PR
    // to get the count of PRs created in the last X days. The GQL `search` query returns a type with the relevant `count` irregardless.
    // The reason this query is so much faster to count PRs is b/c it's just making the one request and not iterating through any data
    let query_string = format!(
        r#"query {{
            search(
                query:"repo:{org_name}/{repo_name} is:pr created:>{date_str}",
                type:ISSUE,
                last:1,
            ) {{
                issueCount
            }}
        }}"#,
        org_name = org_name,
        repo_name = repo_name,
        date_str = date_str,
    );

    let octo = octocrab::instance();
    let response: Response<Data> = octo.graphql(&query_string).await?;

    response.data.search.issue_count as usize
}
