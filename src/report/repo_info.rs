use std::{collections::HashMap, path::Path};

use fehler::throws;
use serde::Deserialize;
use stable_eyre::eyre::{Error, WrapErr};

use crate::{metrics, util::percentage};

use super::{repo_participant::RepoParticipant, Report, ReportConfig};

#[derive(Clone, Debug, Deserialize)]
pub struct RepoInfos {
    pub repos: HashMap<String, RepoInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RepoInfo {
    /// row number
    #[serde(rename = "#")]
    pub row: usize,
    /// name of the GitHub Organization to
    /// which this Repo belongs
    #[serde(rename = "Organization")]
    pub org: String,
    /// the Repository name
    #[serde(rename = "Repository")]
    pub repo: String,
    /// number of PRs opened in the relevant time span
    #[serde(rename = "PRs Opened")]
    pub num_prs: u64,
    /// number of issues opened in the relevant time span
    #[serde(rename = "Issues Opened")]
    pub num_opened: u64,
    /// number of issues closed in the relevant time span
    #[serde(rename = "Issues Closed")]
    pub num_closed: u64,
    /// the starting date of the relevant time span
    #[serde(rename = "Start Date")]
    pub start: String,
    /// the ending date of the relevant time span
    #[serde(rename = "End Date")]
    pub end: String,
}

impl Report {
    /// Produces input data in `$DATA_DIR/inputs/repo-infos.csv` that
    /// will be used as input data in several metrics.
    #[throws]
    pub(super) async fn repo_infos(&self, config: &ReportConfig) -> RepoInfos {
        let repo_infos = self.input_dir().join("repo-infos.csv");

        let graphql = self.graphql("repo-infos");

        self.produce_input(
            &repo_infos,
            metrics::ListReposForOrg::new(
                graphql,
                config.github.org.clone(),
                config.github.repos.clone(),
                config.data_source.start_date.clone(),
                config.data_source.end_date.clone(),
            ),
        )
        .await
        .wrap_err("Failed to produce input data for repo-infos.csv")?;

        tokio::task::spawn_blocking(move || RepoInfos::parse_repo_infos(&repo_infos.clone()))
            .await
            .wrap_err("Failed to spawn blocking task")?
            .wrap_err("Failed to parse repo information")?
    }
}

impl RepoInfos {
    #[throws]
    fn parse_repo_infos(repo_infos: &Path) -> RepoInfos {
        let mut rdr = csv::Reader::from_path(repo_infos)
            .wrap_err_with(|| format!("Failed to create reader from path: {:?}", &repo_infos))?;
        let mut map = HashMap::new();
        for result in rdr.deserialize() {
            let record: RepoInfo =
                result.wrap_err("Failed to deserialize while parsing repo info")?;
            map.insert(record.repo.clone(), record);
        }
        RepoInfos { repos: map }
    }

    pub(super) fn get(&self, repo: &str) -> &RepoInfo {
        &self.repos[repo]
    }
}

impl RepoInfo {
    pub(super) fn is_high_contributor(
        &self,
        config: &ReportConfig,
        participant: &RepoParticipant,
    ) -> bool {
        let hc = &config.high_contributor;

        let participated_in_percentage = percentage(participant.participated_in, self.num_prs);
        let authored_percentage = percentage(participant.authored, self.num_prs);
        let reviewed_or_resolved_percentage =
            percentage(participant.reviewed_or_resolved(), self.num_prs);

        // Identify "high" reviewers or active people.
        let high_reviewer = reviewed_or_resolved_percentage > hc.high_reviewer_min_percentage
            || participant.reviewed_or_resolved() > hc.high_reviewer_min_prs;
        let high_activity = participated_in_percentage > hc.high_participant_min_percentage
            && participant.participated_in > hc.high_participant_min_prs;
        let high_author = authored_percentage > hc.high_author_min_percentage
            && participant.authored > hc.high_author_min_prs;
        let high_total = high_reviewer as u64 + high_activity as u64 + high_author as u64;

        // Being "highly active" in more ways than one makes you a high contributor.
        high_total >= hc.high_contributor_categories_threshold
    }
}
