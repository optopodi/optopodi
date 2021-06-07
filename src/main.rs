use anyhow::Error;
use chrono::{Duration, Utc};
use fehler::throws;
use octocrab::models::Repository;
mod token;
mod util;

#[throws]
#[tokio::main]
async fn main() {
    let token = token::github_token()?;
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()?;

    let rust_lang_org = octocrab.orgs("rust-lang");
    let repos: Vec<Repository> = all_repos(&&rust_lang_org).await?;

    println!("# PRs,\tREPO\n--------------------");
    for repo in &repos {
        let count_prs = count_pull_requests(&octocrab, &repo.name).await?;
        println!("{},\t{}", count_prs, repo.name);
    }
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}

/// count the number of pull requests created in the last 30 days for the given rust-lang repository within the [`rust-lang` GitHub Organization]
///
/// # Arguments
///
/// - `octo` The instance of `octocrab::Octocrab` that should be used to make queries to GitHub API
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the [`rust-lang` Github Organization]
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
/// const num_pull_requests = github-metrics::count_pull_requests(octocrab_instance, String::from("rust"));
///
/// println!("The 'rust-lang/rust' repo has had {} Pull Requests created in the last 30 days!", num_pull_requests);
/// ```
///
/// [`rust-lang` GitHub Organization]: https://github.com/rust-lang
#[throws]
async fn count_pull_requests(octo: &octocrab::Octocrab, repo_name: &String) -> usize {
    let mut page = octo
        .pulls("rust-lang", repo_name)
        .list()
        .sort(octocrab::params::pulls::Sort::Created)
        .per_page(255)
        .send()
        .await?;

    let thirty_days_ago = Utc::now() - Duration::days(30);
    let mut pr_count: usize = 0;

    loop {
        let in_last_thirty_days = page
            .items
            .iter()
            .take_while(|pr| pr.created_at < thirty_days_ago)
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
