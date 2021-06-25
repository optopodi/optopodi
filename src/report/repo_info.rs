use anyhow::Error;
use fehler::throws;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

use crate::metrics::{self, Graphql};

use super::{Report, ReportConfig};

#[derive(Debug, Deserialize)]
pub struct RepoInfos {
    pub repos: HashMap<String, RepoInfo>,
}

#[derive(Debug, Deserialize)]
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

        let graphql_dir = self.graphql_dir().join("repo-infos");
        let graphql = Graphql::new(graphql_dir, self.replay_graphql);

        self.produce_input(
            &repo_infos,
            metrics::ListReposForOrg::new(
                graphql,
                config.github.org.clone(),
                config.github.repos.clone(),
                config.data_source.number_of_days,
            ),
        )
        .await?;

        tokio::task::spawn_blocking(move || RepoInfos::parse_repo_infos(&repo_infos)).await??
    }
}

impl RepoInfos {
    #[throws]
    fn parse_repo_infos(repo_infos: &Path) -> RepoInfos {
        let mut rdr = csv::Reader::from_path(repo_infos)?;
        let mut map = HashMap::new();
        for result in rdr.deserialize() {
            let record: RepoInfo = result?;
            map.insert(record.repo.clone(), record);
        }
        RepoInfos { repos: map }
    }
}
