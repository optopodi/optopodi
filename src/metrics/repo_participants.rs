use std::collections::{HashMap, HashSet};

use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use graphql_client::GraphQLQuery;
use tokio::sync::mpsc::Sender;

use super::{util, Graphql, Producer};

pub struct RepoParticipants {
    graphql: Graphql,
    org_name: String,
    repo_name: Option<String>,
    number_of_days: i64,
}

impl RepoParticipants {
    pub fn new(
        graphql: Graphql,
        org_name: String,
        repo_name: Option<String>,
        number_of_days: i64,
    ) -> Self {
        Self {
            graphql,
            org_name,
            repo_name,
            number_of_days,
        }
    }
}

#[async_trait]
impl Producer for RepoParticipants {
    fn column_names(&self) -> Vec<String> {
        vec![
            String::from("Participant"),
            String::from("Repository"),
            String::from("PRs participated in"),
            String::from("PRs authored"),
            String::from("PRs reviewed"),
            String::from("PRs resolved"),
        ]
    }

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), anyhow::Error> {
        // If no repository is given, repeat for all repositories.
        let repo_names = match &self.repo_name {
            Some(n) => vec![n.to_string()],
            None => util::all_repos(&self.graphql, &self.org_name).await?,
        };

        for repo_name in repo_names {
            let data = pr_participants(
                &self.graphql,
                &self.org_name,
                &repo_name,
                Duration::days(self.number_of_days),
            )
            .await?;

            // FIXME -- there must be some way to "autoderive" this from
            // the `ParticipantCounts` data structure, maybe with serde?
            for (
                login,
                ParticipantCounts {
                    participated_in,
                    authored,
                    reviewed,
                    resolved,
                },
            ) in data
            {
                tx.send(vec![
                    login,
                    repo_name.clone(),
                    participated_in.to_string(),
                    authored.to_string(),
                    reviewed.to_string(),
                    resolved.to_string(),
                ])
                .await?;
            }
        }

        Ok(())
    }
}

#[derive(Default)]
struct ParticipantCounts {
    participated_in: u64,
    authored: u64,
    reviewed: u64,
    resolved: u64,
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
    graphql: &Graphql,
    org_name: &str,
    repo_name: &str,
    time_period: Duration,
) -> Vec<(String, ParticipantCounts)> {
    // get date string to match GitHub's PR query format for `created` field
    // i.e., "2021-05-18UTC" turns into "2021-05-18"
    let date_str = chrono::NaiveDate::parse_from_str(
        &format!("{}", (Utc::now() - time_period).date())[..],
        "%FUTC",
    )
    .unwrap();

    // Tracks, for each github login, how many PRs they participated in on this repository.
    let mut counts: HashMap<String, ParticipantCounts> = HashMap::new();

    let mut after_cursor = None;

    loop {
        let response = graphql
            .query(PrsAndParticipants)
            .execute(pap::Variables {
                query_string: format!(
                    r#"repo:{org_name}/{repo_name} is:pr created:>{date_str}"#,
                    org_name = org_name,
                    repo_name = repo_name,
                    date_str = date_str,
                ),
                after_cursor,
            })
            .await?;
        let response_data = response.data.expect("missing response data");
        for pr_edge in response_data.search.edges.into_iter().flatten().flatten() {
            let pr = match pr_edge.node {
                Some(pap::PrsAndParticipantsSearchEdgesNode::PullRequest(pr)) => pr,
                _ => continue,
            };

            // Extract PR author
            let mut author = None;
            if let Some(a) = pr.author {
                if let pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestAuthorOn::User(u) = a.on {
                    author = Some(u.login);
                }
            }
            let is_author = |s: &str| author.iter().any(|a| a == s);

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
                if !is_author(&login) {
                    counts.entry(login).or_default().participated_in += 1;
                }
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

            let reviewers: HashSet<_> = reviews
                .nodes
                .into_iter()
                .flatten()
                .inspect(|_| reviews_found += 1)
                .flatten()
                .flat_map(|n| n.author)
                .flat_map(|a| {
                    match a.on {
                    pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestReviewsNodesAuthorOn::User(
                        u,
                    ) => Some(u.login),
                    _ => None,
                }
                })
                .collect();
            for reviewer in reviewers {
                // you don't count as a reviewer if you review your own PR
                if !is_author(&reviewer) {
                    counts.entry(reviewer).or_default().reviewed += 1;
                }
            }

            if reviews_found != reviews.total_count {
                anyhow::bail!("FIXME: pagination support for participants list");
            }

            // Count the number of PRs which a person has authored.
            if let Some(a) = author {
                counts.entry(a).or_default().authored += 1;
            }

            // Count the number of PRs which a person has merged.
            if let Some(a) = pr.merged_by {
                if let pap::PrsAndParticipantsSearchEdgesNodeOnPullRequestMergedByOn::User(u) = a.on
                {
                    counts.entry(u.login).or_default().resolved += 1;
                }
            }
        }

        if response_data.search.page_info.has_next_page {
            after_cursor = response_data.search.page_info.end_cursor;
        } else {
            break;
        }
    }

    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|(login, p)| (u64::MAX - p.participated_in, login.clone()));
    counts
}
