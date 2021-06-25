use std::path::Path;
use std::{fs::File, path::PathBuf};

use anyhow::Error;
use fehler::throws;
use serde::Deserialize;

use crate::metrics;
use crate::metrics::Consumer;

mod repo_info;
mod repo_participant;

pub struct Report {
    /// Directory where to store the data.
    data_dir: PathBuf,

    /// If true, load the saved graphql queries from disk.
    replay_graphql: bool,
}

#[derive(Debug, Deserialize)]
struct ReportConfig {
    github: GithubConfig,
    high_contributor: HighContributorConfig,
    data_source: DataSourceConfig,
}

#[derive(Deserialize, Debug)]
struct GithubConfig {
    org: String,
    repos: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct DataSourceConfig {
    number_of_days: i64,
}

#[derive(Deserialize, Debug)]
struct HighContributorConfig {
    /// Number of categories one must be "high" in
    /// to be considered a "high contributor".
    high_reviewer_min_percentage: u64,

    high_reviewer_min_prs: u64,

    reviewer_saturation_threshold: u64,

    author_saturation_threshold: u64,

    high_participant_min_percentage: u64,

    high_participant_min_prs: u64,

    high_author_min_percentage: u64,

    high_author_min_prs: u64,

    /// Number of categories one must be "high" in
    /// to be considered a "high contributor".
    high_contributor_categories_threshold: u64,
}

impl Report {
    pub fn new(data_dir: PathBuf, replay_graphql: bool) -> Self {
        Report {
            data_dir,
            replay_graphql,
        }
    }

    #[throws]
    pub async fn run(mut self) {
        // Load the report configuration from the data directory.
        let config = self.load_config().await?;

        tokio::fs::create_dir_all(self.graphql_dir()).await?;
        tokio::fs::create_dir_all(self.input_dir()).await?;
        tokio::fs::create_dir_all(self.output_dir()).await?;

        let repo_participants = self.repo_participants(&config).await?;
        let repo_infos = self.repo_infos(&config).await?;
        eprintln!("{:#?}", repo_participants);
        eprintln!("{:#?}", repo_infos);
    }

    #[throws]
    async fn load_config(&mut self) -> ReportConfig {
        let report_config_file = self.data_dir.join("report.toml");
        let report_config_bytes = tokio::fs::read_to_string(report_config_file).await?;
        toml::from_str(&report_config_bytes)?
    }

    fn graphql_dir(&self) -> PathBuf {
        self.data_dir.join("graphql")
    }

    fn input_dir(&self) -> PathBuf {
        self.data_dir.join("inputs")
    }

    fn output_dir(&self) -> PathBuf {
        self.data_dir.join("output")
    }

    #[throws]
    async fn produce_input(&self, path: &Path, producer: impl metrics::Producer + Send + 'static) {
        let (column_names, mut rx) = metrics::run_producer(producer);
        let f = File::create(path)?;
        metrics::Print::new(f)
            .consume(&mut rx, column_names)
            .await?;
    }
}
