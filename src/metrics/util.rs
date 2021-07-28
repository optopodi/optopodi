use fehler::throws;
use graphql_client::GraphQLQuery;
use log::debug;
use stable_eyre::eyre::Error;
use toml::value::Datetime;

use super::Graphql;

/// A struct representation of the GraphQL query found in [`gql/organization_repos.graphql`](~/gql/organization_repos.graphql)
///
/// Used to gather relevant data for each repository within a specific GitHub organization.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/organization_repos.graphql",
    response_derives = "Serialize,Debug"
)]
struct OrgRepos;

#[throws]
pub async fn all_repos(graphql: &mut Graphql, org: &str) -> Vec<String> {
    let org_name = format!("{}", org);
    let mut repos: Vec<String> = vec![];
    let mut after_cursor = None;

    loop {
        let res = graphql
            .query(OrgRepos)
            .execute(org_repos::Variables {
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

/// A struct representation of the GraphQL query found in `gql/count_issues.graphql`
///
/// Used to count total number of issues that match the given `query_string`
///
/// Note: Pull Requests and Issues both fall under this umbrella
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/count_issues.graphql",
    response_derives = "Serialize,Debug"
)]
pub(crate) struct CountIssues;

impl CountIssues {
    /// Makes a query to the `gql/count_issues.graphql` query given the relevant `query_string` variable.
    ///
    /// Returns the total number of issues that match the given `query_string`.
    ///
    /// # Arguments
    /// - `graphql` — A `graphql_client::GraphQLQuery` instance to make a GQL query
    /// - `query_string` — The relevant `query_string` to pass into the GQL query
    #[throws]
    pub async fn query(graphql: &mut Graphql, query_string: String) -> usize {
        let response = graphql
            .query(Self)
            .execute(count_issues::Variables { query_string })
            .await?;
        let response_data = response.data.expect("missing response data");
        let count = response_data.search.issue_count;
        count as usize
    }
}

/// count the number of pull requests created in the given time period for the given repository within the given GitHub organization
///
/// # Arguments
/// - `graphql` — A `graphql_client::GraphQLQuery` instance to make a GQL query
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` GitHub Organization
/// - `start_date` — The beginning of the relevant time period to search within
/// - `end_date` — The end of the relevant time period to search within
#[throws]
pub(super) async fn count_pull_requests(
    graphql: &mut Graphql,
    org_name: &str,
    repo_name: &str,
    start_date: &Datetime,
    end_date: &Datetime,
) -> usize {
    let query_string = format!(
        r#"repo:{}/{} is:pr created:{}..{}"#,
        org_name, repo_name, start_date, end_date
    );

    CountIssues::query(graphql, query_string).await?
}

/// count the number of issues opened and closed in a given time period
///
/// # Arguments
/// - `graphql` — A `graphql_client::GraphQLQuery` instance to make a GQL query
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` GitHub Organization
/// - `start_date` — The beginning of the relevant time period to search within
/// - `end_date` — The end of the relevant time period to search within
/// - `state` — The state of the issues to count. (i.e., `"created"` or `"closed"`)
#[throws]
pub(super) async fn count_issues(
    graphql: &mut Graphql,
    org_name: &str,
    repo_name: &str,
    start_date: &Datetime,
    end_date: &Datetime,
    state: &str,
) -> usize {
    debug!("Fetching issue closure info for {}/{}", org_name, repo_name);

    let query_string = format!(
        r#"repo:{org_name}/{repo_name} is:issue {state}:{start_date}..{end_date}"#,
        org_name = org_name,
        repo_name = repo_name,
        start_date = start_date,
        end_date = end_date,
        state = state,
    );

    CountIssues::query(graphql, query_string).await?
}
