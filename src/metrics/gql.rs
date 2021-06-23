use async_trait::async_trait;
use graphql_client::{GraphQLQuery, Response};

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
