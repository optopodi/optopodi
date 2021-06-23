use super::GQL;
use anyhow::Error;
use fehler::throws;
use graphql_client::GraphQLQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/organization_repos.graphql",
    response_derives = "Serialize,Debug"
)]
struct OrgRepos;

#[throws]
pub async fn all_repos_graphql(org: &str) -> Vec<String> {
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
