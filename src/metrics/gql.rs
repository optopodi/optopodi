use anyhow::Error;
use fehler::throws;
use graphql_client::{GraphQLQuery, Response};

#[derive(Debug, Default)]
pub struct Graphql {
    export_prefix: Option<String>,
    import_prefix: Option<String>,
}

impl Graphql {
    /// Used to execute a named query. The `query` argument
    /// should be some struct that has a `[derive(GraphQLQuery)]`
    /// attached to it.
    ///
    /// ```ignore
    /// config.query(QueryStruct).execute(query_struct::Variables { ... })
    /// ```
    pub fn query<Q>(&self, query: Q) -> GraphqlAttached<'_, Q>
    where
        Q: GraphQLQuery,
    {
        GraphqlAttached {
            _config: self,
            _query: query,
        }
    }
}

pub struct GraphqlAttached<'me, Q>
where
    Q: GraphQLQuery,
{
    _config: &'me Graphql,
    _query: Q,
}

impl<'me, Q> GraphqlAttached<'me, Q>
where
    Q: GraphQLQuery,
{
    #[throws]
    pub async fn execute(self, variables: Q::Variables) -> Response<Q::ResponseData> {
        let body = Q::build_query(variables);
        octocrab::instance().post("graphql", Some(&body)).await?
    }
}
