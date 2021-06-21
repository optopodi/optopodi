use anyhow::Error;
use async_trait::async_trait;
use fehler::throws;
use graphql_client::GraphQLQuery;
use tokio::sync::mpsc::{Receiver, Sender};

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

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/query_search.graphql",
    response_derives = "Serialize,Debug"
)]
pub struct QuerySearch;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/organization_repos.graphql",
    response_derives = "Serialize,Debug"
)]
pub struct OrgRepos;

#[async_trait]
pub trait GQL {
    async fn graphql_with_params<R: octocrab::FromResponse + Send>(
        &self,
        body: &(impl serde::Serialize + Send + Sync),
    ) -> octocrab::Result<R>;

    async fn graphql_with_query<Q>(variables: Q::Variables) -> octocrab::Result<Q::ResponseData>
    where
        Q::Variables: Send + Sync,
        Q: GraphQLQuery;
}

#[async_trait]
impl GQL for octocrab::Octocrab {
    async fn graphql_with_params<R: octocrab::FromResponse + Send>(
        &self,
        body: &(impl serde::Serialize + Send + Sync),
    ) -> octocrab::Result<R> {
        Ok(self.post("graphql", Some(&body)).await?)
    }

    async fn graphql_with_query<Q>(variables: Q::Variables) -> octocrab::Result<Q::ResponseData>
    where
        Q::Variables: Send + Sync,
        Q: GraphQLQuery,
    {
        let octo = octocrab::instance();
        let q = Q::build_query(variables);
        Ok(octo.post("graphql", Some(&q)).await?)
    }
}

#[throws]
async fn all_repos_graphql(org: &str) -> Vec<String> {
    let org_name = format!("{}", org);
    let octo = octocrab::instance();

    let mut repos: Vec<String> = vec![];
    let mut after_cursor = None;

    loop {
        let q = OrgRepos::build_query(org_repos::Variables {
            org_name: org_name.to_owned(),
            after_cursor,
        });

        let response: graphql_client::Response<org_repos::ResponseData> =
            octo.graphql_with_params(&q).await?;
        let response_data: org_repos::ResponseData = response.data.expect("missing response data");
        let repos_data = if let Some(org_data) = response_data.organization {
            org_data.repositories
        } else {
            break;
        };

        if let Some(edges) = repos_data.edges {
            for edge in edges.iter() {
                if let Some(Some(name)) = edge
                    .as_ref()
                    .map(|e| e.node.as_ref().map(|n| n.name.to_owned()))
                {
                    repos.push(name);
                }
            }
        }

        if repos_data.page_info.has_next_page {
            after_cursor = repos_data.page_info.end_cursor;
        } else {
            break;
        }
    }

    repos
}
