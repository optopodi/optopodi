use std::path::Path;
use std::sync::Arc;
use std::{fs::File, path::PathBuf};

use fehler::throws;
use serde::Deserialize;
use stable_eyre::eyre::{self, Error, WrapErr};
use toml::value::Datetime;

use crate::metrics::Consumer;
use crate::metrics::{self, Graphql};

mod high_contributor;
mod issue_closure;
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
    start_date: Datetime,
    end_date: Datetime,
}

#[derive(Deserialize, Debug)]
struct HighContributorConfig {
    /// Number of categories one must be "high" in
    /// to be considered a "high contributor".
    high_reviewer_min_percentage: u64,
    /// Number of Pull Requests one must review
    /// in order to be considered a "high contributor"
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
    /// Make a new `Report` instance
    ///
    /// # Arguments
    /// - `data_dir` — A path to the directory containing `report.toml`;
    ///    this is where data will be generated
    /// - `replay_graphql` — A boolean indicating whether to load previous GQL response data from disk
    pub fn new(data_dir: PathBuf, replay_graphql: bool) -> Self {
        Report {
            data_dir,
            replay_graphql,
        }
    }

    /// The driving function for the logic side of our app.
    ///
    /// - loads configuration from the data directory
    /// - handle I/O for folder/file creation
    /// - produces relevant input data and its associated files
    /// - generate output data and associated files for each optopodi metric
    #[throws]
    pub async fn run(mut self) {
        // Load the report configuration from the data directory.
        let config = Arc::new(self.load_config().await.wrap_err("Failed to load config")?);

        // attempt to create all relevant directories
        tokio::fs::create_dir_all(self.graphql_dir())
            .await
            .wrap_err("Failed to create GraphQL Directory")?;
        tokio::fs::create_dir_all(self.input_dir())
            .await
            .wrap_err("Failed to create Input Directory")?;
        tokio::fs::create_dir_all(self.output_dir())
            .await
            .wrap_err("Failed to create Output Directory")?;

        // generate relevant input data
        //
        // the following function calls will...
        //   1. make calls to GitHub's API through GraphQL queries; we'll also store
        //      resulting data in disk so we can `--replay-graphql` later
        //   2. write any "input" CSV files that the user may be interested in looking at or tweaking
        //   3. parse said CSV files into typed Rust objects
        //
        // the result is this in-memory database, of sorts, with all of the data we
        // will later use for our customized metrics
        let data = Arc::new(ReportData {
            top_crates: self
                .top_crates(&config)
                .await
                .wrap_err("Failed to parse Top Crates")?,
            repo_participants: self
                .repo_participants(&config)
                .await
                .wrap_err("Failed to gather Repo Participants")?,
            repo_infos: self
                .repo_infos(&config)
                .await
                .wrap_err("Failed to gather Repo Infos")?,
            // issue_closures: self
            //     .issue_closures(&config)
            //     .await
            //     .wrap_err("Failed to gather issue closure info")?,
        });

        // Finally, we call all of our 'write' functions which produce
        // output data in `$DATA_DIR/output/` folder.
        // Each function will handle its own logic for consuming and manipulating data
        tokio::task::spawn_blocking(move || -> eyre::Result<()> {
            self.write_top_crates(&config, &data)
                .wrap_err("Failed to write Top Crates")?;
            self.write_high_contributors(&config, &data)
                .wrap_err("Failed to write High Contributors")?;
            self.write_issue_closures(&config, &data)
                .wrap_err("Failed to write issue closures")?;
            Ok(())
        })
        .await
        .wrap_err("Failed to spawn blocking task while writing metrics output")?
        .wrap_err("Failed to generate output data for metrics")?;
    }

    /// Load and parse the configuration file from `$DATA_DIR/report.toml`
    #[throws]
    async fn load_config(&mut self) -> ReportConfig {
        let report_config_file = self.data_dir.join("report.toml");
        let report_config_bytes = tokio::fs::read_to_string(report_config_file.clone())
            .await
            .wrap_err_with(|| {
                format!(
                    "Failed to read Report Config from path {:?}",
                    report_config_file
                )
            })?;
        let mut config: ReportConfig =
            toml::from_str(&report_config_bytes).wrap_err("Failed to parse Report Config")?;

        // if user specified an empty list of repos in `report.toml`, then assume
        // they wish to analyze all repositories within the specified `organization`
        if config.github.repos.is_empty() {
            let graphql = &mut self.graphql("all-repos");
            config.github.repos = metrics::all_repos(graphql, &config.github.org)
                .await
                .wrap_err("Failed to gather all repos")?;
        }

        config
    }

    /// get a `Graphql` struct given the associated directory where
    /// GQL response data will be stored
    fn graphql(&self, dir_name: &str) -> Graphql {
        let graphql_dir = self.graphql_dir().join(dir_name);
        Graphql::new(graphql_dir, self.replay_graphql)
    }

    /// get the path to the `$DATA_DIR/graphql/` directory
    fn graphql_dir(&self) -> PathBuf {
        self.data_dir.join("graphql")
    }

    /// get the path to the `$DATA_DIR/inputs/` directory
    fn input_dir(&self) -> PathBuf {
        self.data_dir.join("inputs")
    }

    /// get the path to the `$DATA_DIR/outputs/` directory
    fn output_dir(&self) -> PathBuf {
        self.data_dir.join("output")
    }

    /// Produce a properly formatted CSV file with input data and column names
    /// given (1) the path to the file and (2) the producer of the data
    #[throws]
    async fn produce_input(&self, path: &Path, producer: impl metrics::Producer + Send + 'static) {
        let (column_names, mut rx) = metrics::run_producer(producer);
        let f = File::create(&path)
            .wrap_err_with(|| format!("Failed to create file from path {:?}", path))?;
        metrics::Print::new(f)
            .consume(&mut rx, column_names)
            .await
            .wrap_err("Failed to produce report")?;
    }
}
