use std::path::PathBuf;

use fehler::throws;
use graphql_client::{GraphQLQuery, Response};
use serde::Serialize;
use stable_eyre::eyre::Error;

#[derive(Clone, Debug)]
pub struct Graphql {
    graphql_dir: PathBuf,
    counter: usize,
    replay: bool,
}

impl Graphql {
    pub fn new(graphql_dir: PathBuf, replay: bool) -> Self {
        Self {
            graphql_dir,
            replay,
            counter: 0,
        }
    }

    /// Used to execute a named query. The `query` argument
    /// should be some struct that has a `[derive(GraphQLQuery)]`
    /// attached to it.
    ///
    /// ```ignore
    /// config.query(QueryStruct).execute(query_struct::Variables { ... })
    /// ```
    pub fn query<Q>(&mut self, query: Q) -> GraphqlAttached<'_, Q>
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
    config: &'me mut Graphql,
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

        // get a unique integer for this particular request
        let count = self.config.counter;
        self.config.counter += 1;

        // create the directory and a file within it
        tokio::fs::create_dir_all(&self.config.graphql_dir).await?;
        let path = self.config.graphql_dir.join(format!("{}.json", count));

        if !self.config.replay {
            // execute query and save the data to the file
            let response = octocrab::instance().post("graphql", Some(&body)).await?;
            let response_json = serde_json::to_string(&response)?;
            tokio::fs::write(&path, response_json.as_bytes()).await?;
            response
        } else {
            // if replaying, load the data form the file
            log::info!(
                "loading response data from `{}` rather than github",
                path.display()
            );
            let response_json = tokio::fs::read(&path).await?;
            serde_json::from_slice(&response_json)?
        }
    }
}
