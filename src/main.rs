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
use std::io;


#[derive(Debug, Deserialize, Default)]
struct Config {
    github: GithubConfig,
    google_sheet: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GithubConfig {
    org: Option<String>,
    number_of_days: Option<u64>,
    repo: Option<String>,
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

#[derive(Clap, Debug, PartialEq)]
#[clap(setting = AppSettings::ColoredHelp)]
#[clap(name = "gh-metrics")]
struct OctoCli {
    /// name of GitHub organization to analyze
    #[clap(short, long)]
    org: Option<String>,

    /// Export Option: Print to terminal
    #[clap(short, long)]
    print: bool,

    /// Export Option: Export to Google Sheets
    /// args: the ID of the google-sheet to export to
    #[clap(short, long)]
    google_sheet: Option<String>,

    /// name of GitHub repository to analyze
    #[clap(short, long)]
    repo: Option<String>,

    /// the sub-command to run
    #[clap(subcommand)]
    cmd: Option<Cmd>,

    /// Prefix to use for exporting the results of graphql queries
    #[clap(short, long)]
    export_graphql: Option<String>,

    /// Prefix to use for importing the results of graphql queries
    /// rather than hitting github again (useful when debugging).
    #[clap(short, long)]
    import_graphql: Option<String>,
}

#[derive(Clap, Debug, PartialEq)]
enum Cmd {
    /// list all repositories in the given organization and the number of Pull Requests created in the last 30 days
    List,

    /// list all contributors and their contribution details for the given organization/repository in teh last 30 days
    RepoParticipants,
}

#[throws]
#[tokio::main]
async fn main() {
    env_logger::init();

    let config_path = Path::new("metrics.toml");
    let config = if config_path.exists() {
        Config::load(config_path)?
    } else {
        Config::default()
    };
    let token = token::github_token()?;
    // initialize static octocrab API -- call `octocrab::instance()` anywhere to retrieve instance
    octocrab::initialise(octocrab::Octocrab::builder().personal_token(token))?;

    let cli = OctoCli::parse();

    let org_name: String = match (cli.org, config.github.org) {
        (Some(org_name), _) | (None, Some(org_name)) => org_name,
        (None, None) => panic!("no org name given"),
    };

    let num_days: i64 = match config.github.number_of_days {
        Some(n) => n.try_into().unwrap(),
        None => 30,
    };

    let repo: Option<String> = match (cli.repo, config.github.repo) {
        (Some(repo_name), _) | (None, Some(repo_name)) => Some(repo_name),
        (None, None) => None,
    };

    let sheet_id: Option<String> = match (cli.google_sheet, config.google_sheet) {
        (Some(s_id), _) | (None, Some(s_id)) => Some(String::from(s_id)),
        (None, None) => None,
    };

    let mut column_names: Option<Vec<String>> = None;
    let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);

    let graphql = metrics::Graphql::new(&cli.export_graphql, &cli.import_graphql);

    if let Some(cmd) = cli.cmd {
        match cmd {
            Cmd::List => {
                let list_repos = ListReposForOrg::new(graphql, org_name, num_days);
                column_names = Some(list_repos.column_names());

                tokio::spawn(async move {
                    if let Err(e) = list_repos.producer_task(tx).await {
                        println!("Encountered an error while collecting data: {:#?}", e);
                    };
                });
            }
            Cmd::RepoParticipants => {
                let repo_participants = RepoParticipants::new(graphql, org_name, repo, 30);
                column_names = Some(repo_participants.column_names());

                tokio::spawn(async move {
                    if let Err(e) = repo_participants.producer_task(tx).await {
                        println!("Encountered an error while collecting data: {}", e);
                    };
                });
            }
        }
    }

    // if user specified a google sheet ID, they must want to export data to that sheet
    if let Some(sheet_id) = sheet_id {
        if let Err(e) = ExportToSheets::new(&sheet_id)
            .consume(&mut rx, column_names.clone().unwrap())
            .await
        {
            println!("Error exporting to sheets: {}", e);
        }
    }
    // if user specified the print flag, they must want to print to terminal
    if cli.print {
        if let Err(e) = Print::new(io::stdout()).consume(&mut rx, column_names.unwrap()).await {
            println!("Error while printing results: {}", e);
        }
    }
}
