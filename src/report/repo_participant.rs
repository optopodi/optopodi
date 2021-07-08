use std::path::Path;

use crate::metrics;
use crate::report::repo_info::RepoInfo;
use crate::report::Report;
use crate::util::percentage;
use fehler::throws;
use serde::Deserialize;
use stable_eyre::eyre::{Error, WrapErr};

use super::ReportConfig;

#[derive(Debug, Deserialize)]
pub struct RepoParticipants {
    pub participants: Vec<RepoParticipant>,
}

#[derive(Debug, Deserialize)]
pub struct RepoParticipant {
    #[serde(rename = "#")]
    pub row: usize,
    #[serde(rename = "Participant")]
    pub participant: String,
    #[serde(rename = "Repository")]
    pub repo: String,
    #[serde(rename = "PRs participated in")]
    pub participated_in: u64,
    #[serde(rename = "PRs authored")]
    pub authored: u64,
    #[serde(rename = "PRs reviewed")]
    pub reviewed: u64,
    #[serde(rename = "PRs resolved")]
    pub resolved: u64,
}

impl Report {
    #[throws]
    pub(super) async fn repo_participants(&self, config: &ReportConfig) -> RepoParticipants {
        let input_dir = self.input_dir();
        let repo_participants = input_dir.join("repo-participants.csv");
        let graphql = self.graphql("repo-participants");

        self.produce_input(
            &repo_participants,
            metrics::RepoParticipants::new(
                graphql,
                config.github.org.clone(),
                config.github.repos.clone(),
                config.data_source.start_date.clone(),
                config.data_source.end_date.clone(),
            ),
        )
        .await
        .wrap_err_with(|| format!("Failed to produce input data for {:?}", &repo_participants))?;

        tokio::task::spawn_blocking(move || {
            RepoParticipants::parse_participants(&repo_participants)
        })
        .await
        .wrap_err("Failed to parse repo participants")??
    }
}

impl RepoParticipants {
    #[throws]
    fn parse_participants(repo_participants: &Path) -> Self {
        let mut rdr = csv::Reader::from_path(repo_participants).wrap_err_with(|| {
            format!("Failed to create reader from path {:?}", &repo_participants)
        })?;
        let mut vec = Vec::new();
        for result in rdr.deserialize() {
            let record: RepoParticipant =
                result.wrap_err("Failed to deserialize while parsing repo participants")?;
            if !is_robot(&record.participant) {
                vec.push(record);
            }
        }
        Self { participants: vec }
    }

    /// Finds the participant with the maximum value for `key`.
    pub fn top_participant(
        &self,
        repo_info: &RepoInfo,
        key: impl Fn(&RepoParticipant) -> u64,
    ) -> (String, u64) {
        match self.in_repo(repo_info).max_by_key(|p| key(p)) {
            Some(p) => (p.participant.clone(), percentage(key(p), repo_info.num_prs)),
            None => ("N/A".to_string(), 0),
        }
    }

    /// Finds the participant with the maximum value for `key`.
    pub fn in_repo<'me>(
        &'me self,
        repo_info: &'me RepoInfo,
    ) -> impl Iterator<Item = &'me RepoParticipant> + 'me {
        self.participants
            .iter()
            .filter(move |p| p.repo == repo_info.repo)
    }
}

impl RepoParticipant {
    pub(super) fn reviewed_or_resolved(&self) -> u64 {
        self.reviewed.max(self.resolved)
    }
}

fn is_robot(login: &str) -> bool {
    // FIXME: move to configuration
    const ROBOTS: &[&str] = &[
        "rust-highfive",
        "bors",
        "rustbot",
        "rust-log-analyzer",
        "rust-timer",
        "rfcbot",
    ];

    ROBOTS.contains(&login)
}
