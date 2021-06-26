use std::path::Path;
use std::sync::Arc;
use std::{fs::File, path::PathBuf};

use fehler::throws;
use serde::Deserialize;
use stable_eyre::eyre;
use stable_eyre::eyre::Error;

use crate::metrics::Consumer;
use crate::metrics::{self, Graphql};

mod high_contributor;
mod repo_info;
mod repo_participant;
mod top_crates;

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

#[derive(Debug)]
pub struct ReportData {
    repo_participants: repo_participant::RepoParticipants,
    repo_infos: repo_info::RepoInfos,
    top_crates: Vec<top_crates::TopCrateInfo>,
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
        let config = Arc::new(self.load_config().await?);

        tokio::fs::create_dir_all(self.graphql_dir()).await?;
        tokio::fs::create_dir_all(self.input_dir()).await?;
        tokio::fs::create_dir_all(self.output_dir()).await?;

        let data = Arc::new(ReportData {
            top_crates: self.top_crates(&config).await?,
            repo_participants: self.repo_participants(&config).await?,
            repo_infos: self.repo_infos(&config).await?,
        });

        tokio::task::spawn_blocking(move || -> eyre::Result<()> {
            self.write_top_crates(&config, &data)?;
            self.write_high_contributors(&config, &data)?;
            Ok(())
        })
        .await??;
    }

    #[throws]
    async fn load_config(&mut self) -> ReportConfig {
        let report_config_file = self.data_dir.join("report.toml");
        let report_config_bytes = tokio::fs::read_to_string(report_config_file).await?;
        let mut config: ReportConfig = toml::from_str(&report_config_bytes)?;

        if config.github.repos.is_empty() {
            let graphql = &mut self.graphql("all-repos");
            config.github.repos = metrics::all_repos(graphql, &config.github.org).await?;
        }

        config
    }

    fn graphql(&self, dir_name: &str) -> Graphql {
        let graphql_dir = self.graphql_dir().join(dir_name);
        Graphql::new(graphql_dir, self.replay_graphql)
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
