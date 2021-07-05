use fehler::throws;
use serde::Deserialize;
use stable_eyre::eyre::{Error, WrapErr};
use std::{collections::HashMap, path::Path};

use crate::{metrics, util::percentage};

use super::{repo_participant::RepoParticipant, Report, ReportConfig};

#[derive(Clone, Debug, Deserialize)]
pub struct RepoInfos {
    pub repos: HashMap<String, RepoInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RepoInfo {
    #[serde(rename = "#")]
    pub row: usize,
    #[serde(rename = "Repository Name")]
    pub repo: String,
    #[serde(rename = "# of PRs")]
    pub num_prs: u64,
}

impl Report {
    #[throws]
    pub(super) async fn repo_infos(&self, config: &ReportConfig) -> RepoInfos {
        let input_dir = self.input_dir();
        let repo_infos = input_dir.join("repo-infos.csv");

        let graphql = self.graphql("repo-infos");

        self.produce_input(
            &repo_infos,
            metrics::ListReposForOrg::new(
                graphql,
                config.github.org.clone(),
                config.github.repos.clone(),
                config.data_source.number_of_days,
            ),
        )
        .await
        .wrap_err("Failed to produce input data for repo-infos.csv")?;

        tokio::task::spawn_blocking(move || RepoInfos::parse_repo_infos(&repo_infos.clone()))
            .await
            .wrap_err("Failed to parse repo information")??
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
