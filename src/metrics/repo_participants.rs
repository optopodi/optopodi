use std::collections::{HashMap, HashSet};

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
        .await?;

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

    // Tracks, for each github login, how many PRs they participated in on this repository.
    let mut participated = HashMap::new();

    // Tracks, for each github login, how many PRs they authored on this repository.
    let mut authored: HashMap<String, u64> = HashMap::new();

    // Tracks, for each github login, how many PRs they reviewed or merged on this repository.
    let mut reviewed_or_merged: HashMap<String, u64> = HashMap::new();

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

        eprintln!("{:#?}", pr);

        // For each person who participated on this PR, increment their
        // entry in the `participated` map.
        //
        // Assumption: a given individual will not appear more than once
        // in this list.
        let mut participants_found = 0;
        for participant in pr
            .participants
            .edges
            .into_iter()
            .flatten()
            .flatten()
            .map(|p| p.node)
            .flatten()
            .inspect(|_| participants_found += 1)
        {
            let login = participant.login;
            *participated.entry(login).or_insert(0) += 1;
        }

        // FIXME: We should eventually support the case that there are more than
        // 100 participants, but for now, just assert that we don't need to deal
        // with pagination. The way I would expect to handle this is to have a separate
        // query in which we identify a PR by its internal ID and walk our way through
        // the list of participants.
        if participants_found != pr.participants.total_count {
            anyhow::bail!("FIXME: pagination support for participants list");
        }

        // Count the number of PRs on which a person has issued a review.
        let reviews = pr.reviews.unwrap();
        let mut reviews_found = 0;

        let reviewers: HashSet<String> = reviews
            .nodes
            .into_iter()
            .flatten()
            .inspect(|_| reviews_found += 1)
            .flatten()
            .flat_map(|n| n.author)
            .flat_map(|a| match a.on {
                pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestReviewsNodesAuthorOn::User(
                    u,
                ) => Some(u.login),
                _ => None,
            })
            .collect();
        for reviewer in reviewers {
            *reviewed_or_merged.entry(reviewer).or_insert(0) += 1;
        }

        if reviews_found != reviews.total_count {
            anyhow::bail!("FIXME: pagination support for participants list");
        }

        // Count the number of PRs which a person has authored.
        if let Some(a) = pr.author {
            if let pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestAuthorOn::User(u) = a.on {
                *authored.entry(u.login).or_insert(0) += 1;
            }
        }

        // Count the number of PRs which a person has merged.
        if let Some(a) = pr.merged_by {
            if let pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestMergedByOn::User(u) = a.on {
                *reviewed_or_merged.entry(u.login).or_insert(0) += 1;
            }
        }
    }

    let mut counts = vec![];
    for (login, prs) in participated {
        counts.push(Participant { login, prs });
    }

    counts.sort_by_key(|p| u64::MAX - p.prs);

    counts
}
