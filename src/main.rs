use anyhow::Error;
use clap::{AppSettings, Clap};
use fehler::throws;
use serde_derive::Deserialize;
use std::convert::TryInto;
use std::path::Path;
use tokio::sync::mpsc;

mod google_sheets;
mod metrics;
mod token;

use metrics::{Consumer, ExportToSheets, ListReposForOrg, Print, Producer, RepoParticipants};

#[derive(Debug, Deserialize, Default)]
struct Config {
    github: GithubConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct GithubConfig {
    org: Option<String>,
    number_of_days: Option<u64>,
}

impl Config {
    #[throws(anyhow::Error)]
    pub fn load(path: &Path) -> Config {
        let config_text = std::fs::read_to_string(path)?;
        Self::parse(&config_text)?
    }

    #[throws(anyhow::Error)]
    pub fn parse(text: &str) -> Config {
        toml::from_str(text)?
    }
}

#[derive(Clap, Debug)]
#[clap(setting = AppSettings::ColoredHelp)]
#[clap(name = "gh-metrics")]
enum Opt {
    /// list all repositories in the given organization and the number of
    /// Pull Requests created in the last 30 days
    List {
        /// name of GitHub organization to analyze
        #[clap(short, long)]
        org: String,

        /// Verbose mode (-v, -vv, -vvv, etc.)
        #[clap(short, long, parse(from_occurrences))]
        verbose: u8,

        #[clap(short, long)]
        google_sheet: Option<String>,
    },

    /// list all repositories in the given organization and the number of
    /// Pull Requests created in the last 30 days
    RepoParticipants {
        /// name of GitHub organization to analyze
        #[clap(short, long)]
        org: String,

        /// name of GitHub organization to analyze
        #[clap(short, long)]
        repo: Option<String>,

        /// Verbose mode (-v, -vv, -vvv, etc.)
        #[clap(short, long, parse(from_occurrences))]
        verbose: u8,
    },
}

#[throws]
#[tokio::main]
async fn main() {
    let config_path = Path::new("metrics.toml");
    let config = if config_path.exists() {
        Config::load(config_path)?
    } else {
        Config::default()
    };
    let token = token::github_token()?;
    // initialize static octocrab API -- call `octocrab::instance()` anywhere to retrieve instance
    octocrab::initialise(octocrab::Octocrab::builder().personal_token(token))?;

    match Opt::parse() {
        Opt::List {
            org,
            google_sheet: gs,
            verbose: _,
        } => {
            let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);
            let org_name = if let Some(org_name) = config.github.org {
                org_name
            } else {
                org
            };
            let num_days: i64 = if let Some(number_of_days) = config.github.number_of_days {
                number_of_days.try_into().unwrap()
            } else {
                30
            };

            let list_repos = ListReposForOrg::new(org_name, num_days);
            let column_names = list_repos.column_names();

            tokio::spawn(async move {
                if let Err(e) = list_repos.producer_task(tx).await {
                    println!("Encountered an error while collecting data: {}", e);
                };
            });

            match gs {
                // if user specified a google sheet ID, they must want to export data to that sheet
                Some(sheet_id) => {
                    if let Err(e) = ExportToSheets::new(&sheet_id)
                        .consume(&mut rx, column_names)
                        .await
                    {
                        println!("Error exporting to sheets: {}", e);
                    }
                }
                // User wants to print to terminal, they did not specify a google sheet ID
                None => {
                    if let Err(e) = Print::consume(Print, &mut rx, column_names).await {
                        println!("Error while printing results: {}", e);
                    }
                }
            }
        }

        Opt::RepoParticipants {
            org,
            repo,
            verbose: _,
        } => {
            let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);
            let repo_participants = RepoParticipants::new(org, repo, 30);
            let column_names = repo_participants.column_names();

            tokio::spawn(async move {
                if let Err(e) = repo_participants.producer_task(tx).await {
                    println!("Encountered an error while collecting data: {}", e);
                };
            });

            if let Err(e) = Print::consume(Print, &mut rx, column_names).await {
                println!("Error while printing results: {}", e);
            }
        }
    }
}
