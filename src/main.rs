use anyhow::Error;
use chrono::{Duration, Utc};
use clap::{AppSettings, Clap};
use fehler::throws;
use octocrab::models::Repository;
use serde_derive::Deserialize;
use std::path::Path;
use tokio::sync::mpsc::{self, Receiver, Sender};

mod google_sheets;
mod token;
mod util;

use google_sheets::Sheets;

#[derive(Debug, Deserialize, Default)]
struct Config {
    github: GithubConfig,
}

#[derive(Debug, Deserialize, Default)]
struct GithubConfig {
    org: String,
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
}

#[throws]
#[tokio::main]
async fn main() {
    let config_path = Path::new("metrics.toml");
    let _config = if config_path.exists() {
        Config::load(config_path)?
    } else {
        Config::default()
    };
    let token = token::github_token()?;
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()?;

    match Opt::parse() {
        Opt::List {
            org,
            google_sheet: gs,
            verbose: _,
        } => {
            // utilize `tokio::sync::mpsc` to decouple producing and consuming data
            // `tx` can be used for sending entries (producing data)
            // `rx` can be used to DO something with gathered data (consuming data)
            let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);

            // produce all relevant data within this
            tokio::spawn(async move {
                if let Err(error_message) = gather_list_data(&tx, &octocrab, &org).await {
                    println!("{}", error_message);
                }
            });

            match gs {
                // if user specified a google sheet ID, they must want to export data to that sheet
                Some(sheet_id) => export_list_data_to_google_sheet(&mut rx, &sheet_id).await,
                // User wants to print to terminal, they did not specify a google sheet ID
                None => {
                    println!("# of PRs\tRepository Name\n------------------------------");
                    while let Some(entry) = rx.recv().await {
                        println!("{}\t{}", &entry[1], &entry[0]);
                    }
                }
            }
        }
    }
}

/// Given a bounded single-consumer `Receiver` which holds relevant data, export each
/// data entry to the Google Sheets instance with the associated `sheet_id`
async fn export_list_data_to_google_sheet(rx: &mut Receiver<Vec<String>>, sheet_id: &str) {
    let sheets = match Sheets::initialize(&sheet_id).await {
        Ok(s) => s,
        Err(e) => {
            println!("There's been an error! {}", e);
            return;
        }
    };

    // clear existing data from sheet
    if let Err(e) = sheets.clear_sheet().await {
        println!("There's been an error clearing the sheet: {}", e);
    }

    // add headers / column titles
    if let Err(e) = sheets
        .append(vec![
            String::from("Repository Name"),
            String::from("# of PRs"),
        ])
        .await
    {
        println!("There's been an error appending the column names {}", e);
    }

    // wait for `tx` to send data
    while let Some(data) = rx.recv().await {
        let user_err_message = format!("Had trouble appending repo {}", &data[0]);
        if let Err(e) = sheets.append(data).await {
            println!("{}: {}", user_err_message, e);
        }
    }
    println!(
        "Finished exporting data to sheet: {}",
        sheets.get_link_to_sheet()
    );
}

/// Given a bounded multi-producer `Sender`, gather and send all relevant data
/// for the `Opt::List` command. This currently includes:
/// - each repository within the given GitHub Organization
/// - and the number of Pull Requests created in the past 30 days.
async fn gather_list_data(
    tx: &Sender<Vec<String>>,
    octo: &octocrab::Octocrab,
    org_name: &str,
) -> Result<(), String> {
    let gh_org = octo.orgs(org_name);

    let repos: Vec<Repository> = match all_repos(&gh_org).await {
        Ok(r) => r,
        Err(e) => {
            return Err(format!(
                "Ran into an error while gathering repositories! {}",
                e
            ))
        }
    };

    for repo in &repos {
        match count_pull_requests(&octo, &org_name, &repo.name).await {
            Ok(count_prs) => {
                if let Err(_) = tx
                    .send(vec![String::from(&repo.name), count_prs.to_string()])
                    .await
                // here we send the entry in `[repository_name, count_prs]` format
                {
                    return Err(String::from("receiver dropped!"));
                }
            }

            Err(e) => {
                return Err(format!(
                    "Ran into an issue while counting PRs for repository {}: {}",
                    &repo.name, e
                ))
            }
        }
    }

    Ok(())
}

#[throws]
async fn all_repos(org: &octocrab::orgs::OrgHandler<'_>) -> Vec<octocrab::models::Repository> {
    util::accumulate_pages(|page| org.list_repos().page(page).send()).await?
}

/// count the number of pull requests created in the last 30 days for the given repository within the given GitHub organization
///
/// # Arguments
///
/// - `octo` — The instance of `octocrab::Octocrab` that should be used to make queries to GitHub API
/// - `org_name` — The name of the github organization that owns the specified repository
/// - `repo_name` — The name of the repository to count pull requests for. **Note:** repository should exist within the `org_name` Github Organization
///
/// # Example
///
/// ```
/// use github-metrics;
/// use octocrab;
/// use std::string::String;
///
/// let octocrab_instance = octocrab::Octocrab::builder().personal_token("SOME_GITHUB_TOKEN").build()?;
///
/// const num_pull_requests = github-metrics::count_pull_requests(octocrab_instance, "rust-lang", "rust");
///
/// println!("The 'rust-lang/rust' repo has had {} Pull Requests created in the last 30 days!", num_pull_requests);
/// ```
#[throws]
async fn count_pull_requests(octo: &octocrab::Octocrab, org_name: &str, repo_name: &str) -> usize {
    let mut page = octo
        .pulls(org_name, repo_name)
        .list()
        // take all PRs -- open or closed
        .state(octocrab::params::State::All)
        // sort by date created
        .sort(octocrab::params::pulls::Sort::Created)
        // start with most recent
        .direction(octocrab::params::Direction::Descending)
        .per_page(255)
        .send()
        .await?;

    let thirty_days_ago = Utc::now() - Duration::days(30);
    let mut pr_count: usize = 0;

    loop {
        let in_last_thirty_days = page
            .items
            .iter()
            // take while PRs have been created within the past thirty days
            .take_while(|pr| pr.created_at > thirty_days_ago)
            .count();

        pr_count += in_last_thirty_days;
        if in_last_thirty_days < page.items.len() {
            // No need to visit the next page.
            break;
        }

        if let Some(p) = octo.get_page(&page.next).await? {
            page = p;
        } else {
            break;
        }
    }

    pr_count
}
