use std::collections::HashMap;

use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use graphql_client::GraphQLQuery;
use tokio::sync::mpsc::Sender;

use super::{Producer, GQL};

pub struct RepoParticipants {
    org_name: String,
    repo_name: String,
    number_of_days: i64,
}

impl RepoParticipants {
    pub fn new(org_name: String, repo_name: String, number_of_days: i64) -> Self {
        Self {
            org_name,
            repo_name,
            number_of_days,
        }
    }
}

#[async_trait]
impl Producer for RepoParticipants {
    fn column_names(&self) -> Vec<String> {
        vec![String::from("Participant"), String::from("PRs")]
    }

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error> {
        let data = pr_participants(
            &self.org_name,
            &self.repo_name,
            Duration::days(self.number_of_days),
        )
        .await
        .unwrap();

        for participant in data {
            tx.send(vec![participant.login, participant.prs.to_string()])
                .await?;
        }

        Ok(())
    }
}

struct Participant {
    login: String,
    prs: u64,
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "gql/schema.docs.graphql",
    query_path = "gql/prs_and_participants.graphql",
    response_derives = "Serialize,Debug"
)]
pub struct PrsAndParticipants;
use prs_and_participants as pap;

/// count the number of pull requests created in the given time period for the given repository within the given GitHub organization
///
/// # Arguments
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` Github Organization
/// - `time_period` — The relevant time period to search within
#[throws]
async fn pr_participants(
    org_name: &str,
    repo_name: &str,
    time_period: Duration,
) -> Vec<Participant> {
    // get date string to match GitHub's PR query format for `created` field
    // i.e., "2021-05-18UTC" turns into "2021-05-18"
    let date_str = chrono::NaiveDate::parse_from_str(
        &format!("{}", (Utc::now() - time_period).date())[..],
        "%FUTC",
    )
    .unwrap();

    let query_string = format!(
        r#"repo:{org_name}/{repo_name} is:pr created:>{date_str}"#,
        org_name = org_name,
        repo_name = repo_name,
        date_str = date_str,
    );

    let mut data = HashMap::new();

    let response = PrsAndParticipants::execute(pap::Variables {
        query_string,
        after_cursor: None,
    })
    .await?;
    let response_data = response.data.expect("missing response data");
    for pr_edge in response_data.search.edges.into_iter().flatten().flatten() {
        let _cursor = pr_edge.cursor;
        let pr = match pr_edge.node {
            Some(pap::PrsAndParticipantsSearchEdgesNode::PullRequest(pr)) => pr,
            _ => continue,
        };
        let number = pr.number;
        for participant in pr
            .participants
            .edges
            .into_iter()
            .flatten()
            .flatten()
            .map(|p| p.node)
            .flatten()
        {
            let login = participant.login;
            data.entry(login).or_insert(Vec::new()).push(number);
        }
    }

    let mut counts = vec![];
    for (login, pr_numbers) in data {
        counts.push(Participant {
            login,
            prs: pr_numbers.len() as u64,
        });
    }

    counts.sort_by_key(|p| u64::MAX - p.prs);

    counts
}
