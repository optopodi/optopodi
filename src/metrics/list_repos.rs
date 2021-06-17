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

/// count the number of pull requests created in the last 30 days for the given repository within the given GitHub organization
///
/// # Arguments
///
/// - `octo` — The instance of `octocrab::Octocrab` that should be used to make queries to GitHub API
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
async fn count_pull_requests(org_name: &String, repo_name: &str) -> usize {
    let octo = octocrab::instance();
    let mut page = octo
        .pulls(org_name, repo_name)
        .list()
        // take all PRs -- open or closed
        .state(octocrab::params::State::All)
        // sort by date created
        .sort(octocrab::params::pulls::Sort::Created)
        // start with most recent
        .direction(octocrab::params::Direction::Descending)
        .per_page(255)
        .send()
        .await?;

    let thirty_days_ago = Utc::now() - Duration::days(30);
    let mut pr_count: usize = 0;

    loop {
        let in_last_thirty_days = page
            .items
            .iter()
            // take while PRs have been created within the past thirty days
            .take_while(|pr| pr.created_at > thirty_days_ago)
            .count();

        pr_count += in_last_thirty_days;
        if in_last_thirty_days < page.items.len() {
            // No need to visit the next page.
            break;
        }

        if let Some(p) = octo.get_page(&page.next).await? {
            page = p;
        } else {
            break;
        }
    }

    pr_count
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

#[throws]
async fn count_pull_requests_graphql(org_name: &str, repo_name: &str) -> usize {
    let octo = octocrab::instance();
    let s = format!("{}", (Utc::now() - Duration::days(30)).date());
    let mut chars = s.chars();
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
        org_name=org_name,
        thirty_days_ago=thirty_days_ago_str,
        repo_name=repo_name
    );
    let response: Response<Data> = octo.graphql(&query_string).await?;

    response.data.search.issue_count as usize
}
