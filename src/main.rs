use anyhow::Error;
use chrono::{Duration, Utc};
use fehler::throws;
use octocrab::models::{pulls::PullRequest, Repository};
use std::option::Option;
use url::Url;
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

    println!("#PR,\tREPO\n--------------------");
    for repo in &repos {
        let count_prs = count_pull_requests(&octocrab, &repo.name).await?;
        println!("{},\t{}", count_prs, repo.name);
    }
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}

/// count the number of pull requests created in the last 30 days for the given repository within the [`rust-lang` github organization](https://github.com/rust-lang)
///
/// ## Arguments
///
/// - `octo` — the instance of `octocrab::Octocrab` that should be used to make any GitHub queries
/// - `repo_name` — The name of the repository (within GitHub Organization "rust-lang") to count pull-requests for
///
/// ## Example
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
/// println!("'rust-lang/rust' has had {} Pull Requests in the last 30 days!", num_pull_requests);
/// ```
#[throws]
async fn count_pull_requests(octo: &octocrab::Octocrab, repo_name: &String) -> usize {
    let init_page = octo
        .pulls("rust-lang", repo_name)
        .list()
        .sort(octocrab::params::pulls::Sort::Created)
        .per_page(100)
        .send()
        .await?;

    let thirty_days_ago = Utc::now() - Duration::days(30);
    let mut pr_count: usize = 0;
    let mut next_page = init_page.next.to_owned();

    // given a page of PRs, count any valid PRs and stop iteration as soon as we surpass our date range of 30 days
    let mut count_valid_prs = |page: octocrab::Page<PullRequest>| -> Option<Url> {
        let copy_next = page.next.to_owned();
        let mut all_valid = true;
        for pr in page {
            if pr.created_at < thirty_days_ago {
                pr_count += 1;
            } else {
                all_valid = false;
                break;
            }
        }

        return if all_valid { copy_next } else { None };
    };

    // run initial page of data through the counter; this might be the only page we have!
    // note: this is set up this way b/c the `octo.get_page` different is called for all
    // subsequent pages (rather than `octo.pulls()` which allows us all sorts of good specifications)
    count_valid_prs(init_page);

    while let Some(page) = octo.get_page::<PullRequest>(&next_page).await? {
        next_page = count_valid_prs(page);
    }

    pr_count
}
