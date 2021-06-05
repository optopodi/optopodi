use anyhow::Error;
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

    for repo in &repos {
        println!("repo: {}", repo.name);
    }
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}
