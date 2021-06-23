use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Error;
use fehler::throws;
use graphql_client::{GraphQLQuery, Response};
use serde::Serialize;

#[derive(Debug)]
pub struct Graphql {
    export_prefix: Option<String>,
    export_counter: Arc<AtomicUsize>,
    import_prefix: Option<String>,
}

impl Graphql {
    pub fn new(export_prefix: &Option<String>, import_prefix: &Option<String>) -> Self {
        Self {
            export_prefix: export_prefix.clone(),
            import_prefix: import_prefix.clone(),
            export_counter: Default::default(),
        }
    }

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
            config: self,
            _query: query,
        }
    }
}

pub struct GraphqlAttached<'me, Q>
where
    Q: GraphQLQuery,
{
    config: &'me Graphql,
    _query: Q,
}

impl<'me, Q> GraphqlAttached<'me, Q>
where
    Q: GraphQLQuery,
{
    #[throws]
    pub async fn execute(self, variables: Q::Variables) -> Response<Q::ResponseData>
    where
        Q::ResponseData: Serialize,
    {
        let body = Q::build_query(variables);

        let count = self.config.export_counter.fetch_add(1, Ordering::SeqCst);

        if let Some(import_prefix) = &self.config.import_prefix {
            let path = format!("{}.{}.json", import_prefix, count);

            log::info!("loading response data from `{}` rather than github", path);

            let response_json = tokio::fs::read(&path).await?;

            serde_json::from_slice(&response_json)?
        } else {
            let response = octocrab::instance().post("graphql", Some(&body)).await?;

            if let Some(export_prefix) = &self.config.export_prefix {
                let path = format!("{}.{}.json", export_prefix, count);
                let reponse_json = serde_json::to_string(&response)?;
                tokio::fs::write(&path, reponse_json.as_bytes()).await?;
            }

            response
        }
    }
}
