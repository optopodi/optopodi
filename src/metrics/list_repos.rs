use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use octocrab::models::Repository;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use super::Producer;

pub struct ListReposForOrg {
    org_name: String,
}

impl ListReposForOrg {
    pub fn new(org_name: &str) -> Self {
        ListReposForOrg {
            org_name: String::from(org_name),
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![String::from("Repository Name"), String::from("# of PRs")]
    }

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String> {
        let octo = octocrab::instance();
        let gh_org = octo.orgs(&self.org_name);

        let repos: Vec<Repository> = match super::all_repos(&gh_org).await {
            Ok(r) => r,
            Err(e) => {
                return Err(format!(
                    "Ran into an error while gathering repositories! {}",
                    e
                ));
            }
        };

        for repo in &repos {
            match count_pull_requests_graphql(&self.org_name, &repo.name).await {
                Ok(count_prs) => {
                    if let Err(e) = tx
                        .send(vec![String::from(&repo.name), count_prs.to_string()])
                        .await
                    {
                        return Err(format!("{:#?}", e));
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "Ran into an issue while counting PRs for repository {}: {}",
                        &repo.name, e
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct Response<T> {
    data: T,
    errors: Option<Vec<serde_json::Value>>,
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

/// count the number of pull requests created in the last 30 days for the given repository within the given GitHub organization
///
/// # Arguments
///
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` Github Organization
///
/// # Example
///
/// ```
/// use github-metrics;
/// use octocrab;
/// use std::string::String;
///
/// let octocrab_instance = octocrab::Octocrab::builder().personal_token("SOME_GITHUB_TOKEN").build()?;
///
/// const num_pull_requests = github-metrics::count_pull_requests(octocrab_instance, "rust-lang", "rust");
///
/// println!("The 'rust-lang/rust' repo has had {} Pull Requests created in the last 30 days!", num_pull_requests);
/// ```
#[throws]
async fn count_pull_requests_graphql(org_name: &str, repo_name: &str) -> usize {
    let octo = octocrab::instance();

    // the following madness simply removes the "UTC" at the end of the
    // date string to match GitHub's Query
    // i.e., "2021-05-18UTC" turns into "2021-05-18"
    let date_string = format!("{}", (Utc::now() - Duration::days(30)).date());
    let mut chars = date_string.chars();
    chars.next_back();
    chars.next_back();
    chars.next_back();
    let thirty_days_ago_str = chars.as_str();

    let query_string = format!(
        r#"query {{
            search(
                query:"repo:{org_name}/{repo_name} is:pr created:>{thirty_days_ago}",
                type:ISSUE,
                last:100,
            ) {{
                issueCount
            }}
        }}"#,
        org_name = org_name,
        thirty_days_ago = thirty_days_ago_str,
        repo_name = repo_name
    );
    let response: Response<Data> = octo.graphql(&query_string).await?;

    response.data.search.issue_count as usize
}
