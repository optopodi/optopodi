use anyhow::Error;
use chrono::{Duration, Utc};
use fehler::throws;
use octocrab::models::pulls::PullRequest;
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
    let mut pr_count: usize = init_page.items.len();
    let mut next_page = init_page.next.to_owned();

    let mut count_valid_prs = |page: octocrab::Page<PullRequest>| -> Option<()> {
        let mut all_valid = true;
        for pr in page {
            if pr.created_at < thirty_days_ago {
                pr_count += 1;
            } else {
                all_valid = false;
                break;
            }
        }

        return if all_valid { Some(()) } else { None };
    };

    while let Some(page) = octo.get_page::<PullRequest>(&next_page).await? {
        let copy_next = page.next.to_owned();
        next_page = match count_valid_prs(page) {
            Some(_) => copy_next,
            _ => None,
        }
    }

    pr_count
}
