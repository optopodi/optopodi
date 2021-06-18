use anyhow::Error;
use async_trait::async_trait;
use fehler::throws;
use serde::Deserialize;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::util;

mod export_to_sheets;
mod list_repos;
mod print;

#[async_trait]
pub trait Producer {
    fn column_names(&self) -> Vec<String>;
    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String>;
}

#[async_trait]
pub trait Consumer {
    async fn consume(
        self,
        rx: &mut Receiver<Vec<String>>,
        column_names: Vec<String>,
    ) -> Result<(), String>;
}

pub use export_to_sheets::ExportToSheets;
pub use list_repos::ListReposForOrg;
pub use print::Print;

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}

#[derive(Deserialize, Debug)]
pub struct Response<T> {
    data: T,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug)]
struct AllReposData {
    organization: Organization,
}

#[derive(Deserialize, Debug)]
struct Organization {
    repositories: Repositories,
}

#[derive(Deserialize, Debug)]
struct Repositories {
    edges: Vec<Node>,
}

#[derive(Deserialize, Debug)]
struct Node {
    node: RepoNode,
}

#[derive(Deserialize, Debug)]
struct RepoNode {
    name: String,
}

#[throws]
async fn all_repos_graphql(org: &str) -> Vec<String> {
    let octo = octocrab::instance();
    let query_string = format!(
        r#"query {{
            organization(login:"{org_name}"){{
                repositories(first:100){{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
          }}"#,
        org_name = org,
    );

    let response: Response<AllReposData> = octo.graphql(&query_string).await?;

    response
        .data
        .organization
        .repositories
        .edges
        .iter()
        .map(|node| node.node.name.to_owned())
        .collect::<Vec<String>>()
}
