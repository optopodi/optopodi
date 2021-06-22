use anyhow::Error;
use async_trait::async_trait;
use fehler::throws;
use graphql_client::{GraphQLQuery, Response};
use tokio::sync::mpsc::{Receiver, Sender};

mod export_to_sheets;
mod list_repos;
mod print;
mod repo_participants;

#[async_trait]
pub trait Producer {
    fn column_names(&self) -> Vec<String>;
    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error>;
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
pub use repo_participants::RepoParticipants;

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
pub trait GQL: GraphQLQuery {
    async fn execute(variables: Self::Variables) -> octocrab::Result<Response<Self::ResponseData>>;
}

#[async_trait]
impl<Q> GQL for Q
where
    Q: GraphQLQuery,
    Q::Variables: Send + Sync,
{
    async fn execute(variables: Self::Variables) -> octocrab::Result<Response<Self::ResponseData>> {
        let body = Self::build_query(variables);
        octocrab::instance().post("graphql", Some(&body)).await
    }
}

#[throws]
async fn all_repos_graphql(org: &str) -> Vec<String> {
    let org_name = format!("{}", org);
    let mut repos: Vec<String> = vec![];
    let mut after_cursor = None;

    loop {
        let res = OrgRepos::execute(org_repos::Variables {
            org_name: org_name.to_owned(),
            after_cursor,
        })
        .await?;

        let response_data = res.data.expect("missing response data");
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
