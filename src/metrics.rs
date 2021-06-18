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

// ==================== GQL Query Structures =========================

#[derive(Deserialize, Debug)]
pub struct Response<T> {
    data: T,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: String,
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
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Deserialize, Debug)]
struct Node {
    node: RepoNode,
}

#[derive(Deserialize, Debug)]
struct RepoNode {
    name: String,
}

// ==================== GQL Query Functions =========================

/// returns a list of relevant data (only) for each repositories under the given organization.
///
/// Note: Currently, the only repo data we're even using is its name. This will likely change over time.
/// Mutate the query found in `metrics::get_query_str_all_repos` to add any relevant data necessary.
#[throws]
async fn all_repos_graphql(org: &str) -> Vec<String> {
    let octo = octocrab::instance();

    let mut query_string = get_query_str_all_repos(&org, None);
    let mut repos: Vec<String> = vec![];

    loop {
        let response: Response<AllReposData> = octo.graphql(&query_string).await?;
        let repos_data = response.data.organization.repositories;
        repos.extend(
            repos_data
                .edges
                .iter()
                .map(|edge| edge.node.name.to_owned()),
        );

        if repos_data.page_info.has_next_page {
            query_string = get_query_str_all_repos(&org, Some(&repos_data.page_info.end_cursor));
        } else {
            break;
        }
    }

    repos
}

/// utility function used for pagination with the GraphQL "get all repositories for organization" query
///
/// # Arguments
/// - `org` — The name of the GitHub Organization to retrieve all the repositories for
/// - `after_cursor` — An optional cursor value to start at (provided by the `pageInfo` property in the previous page)
///
/// Feel free to explore in the [GitHub GraphQL Explorer]
///
/// [GitHub GraphQL Explorer]: https://docs.github.com/en/graphql/overview/explorer
fn get_query_str_all_repos(org: &str, after_cursor: Option<&str>) -> String {
    format!(
        r#"query {{
            organization(login:"{org_name}"){{
                repositories(first:100{after_clause}){{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        endCursor
                    }}
                }}
            }}
          }}"#,
        org_name = org,
        after_clause = if let Some(cursor) = after_cursor {
            format!(r#", after:"{}""#, cursor)
        } else {
            "".to_string()
        },
    )
}
