use std::future::Future;

use anyhow::Error;
use fehler::throws;
use octocrab::Page;
mod token;

#[throws]
#[tokio::main]
async fn main() {
    let token = token::github_token()?;
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()?;

    let rust_lang_org = octocrab.orgs("rust-lang");
    let repos = all_repos(&&rust_lang_org).await?;

    for repo in &repos {
        println!("repo: {}", repo.name);
    }
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    accumulate_pages(|page| org.list_repos().page(page).send()).await?
}

#[throws]
async fn accumulate_pages<T, F>(mut data: impl FnMut(u32) -> F) -> Vec<T>
where
    F: Future<Output = Result<Page<T>, octocrab::Error>>,
{
    let mut repos = vec![];
    for page in 0_u32.. {
        let page = data(page).await?;
        let items_len = page.items.len();
        repos.extend(page.items);
        if items_len == 0 {
            break;
        }
    }
    return repos;
}
