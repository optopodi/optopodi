use anyhow::Error;
use chrono::{Duration, Utc};
use clap::{AppSettings, Clap};
use fehler::throws;
use octocrab::models::Repository;
mod token;
mod util;

#[derive(Clap, Debug)]
#[clap(setting = AppSettings::ColoredHelp)]
#[clap(name = "gh-metrics")]
enum Opt {
    /// list all repositories in the given organization and the number of
    /// Pull Requests created in the last 30 days
    List {
        /// name of GitHub organization to analyze
        #[clap(short, long)]
        org: String,

        /// Verbose mode (-v, -vv, -vvv, etc.)
        #[clap(short, long, parse(from_occurrences))]
        verbose: u8,
    },
}

#[throws]
#[tokio::main]
async fn main() {
    let token = token::github_token()?;
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()?;

    match Opt::parse() {
        Opt::List { org, verbose: _ } => {
            let gh_org = octocrab.orgs(&org);
            let repos: Vec<Repository> = all_repos(&gh_org).await?;

            println!("# PRs,\tREPO\n--------------------");
            for repo in &repos {
                let count_prs = count_pull_requests(&octocrab, &org, &repo.name).await?;
                println!("{},\t{}", count_prs, repo.name);
            }
        }
    }
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
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
async fn count_pull_requests(octo: &octocrab::Octocrab, org_name: &str, repo_name: &str) -> usize {
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
