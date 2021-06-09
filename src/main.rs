use std::fmt;

use anyhow::Error;
use chrono::{Duration, Utc};
use clap::{AppSettings, Clap};
use fehler::throws;
use octocrab::models::Repository;
use std::io::{self, Write};

mod google_sheets;
mod token;
mod util;

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

struct Collection<T: google_sheets::IntoSheetEntry> {
    headers: Vec<String>,
    data: Vec<T>,
    num_columns: usize,
    num_rows: usize,
}

trait IntoCollection<T: google_sheets::IntoSheetEntry> {
    fn into_entry(data: Vec<T>) -> Collection<T>;
}

#[derive(Debug)]
struct ListEntry {
    repository_name: String,
    number_recent_pull_requests: usize,
}

impl google_sheets::IntoSheetEntry for ListEntry {
    fn into_sheet_entry(&self) -> Vec<String> {
        vec![
            self.number_recent_pull_requests.to_string(),
            String::from(&self.repository_name),
        ]
    }
}

impl fmt::Display for ListEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},\t\t{}",
            self.number_recent_pull_requests, self.repository_name
        )
    }
}

impl IntoCollection<ListEntry> for ListEntry {
    fn into_entry(data: Vec<ListEntry>) -> Collection<ListEntry> {
        Collection::new(vec!["# of PRs\t", "Repository Name"], data)
    }
}

impl<T: std::fmt::Display + google_sheets::IntoSheetEntry> Collection<T> {
    fn new(headers: Vec<&str>, data: Vec<T>) -> Self {
        Collection {
            num_columns: headers.len(),
            num_rows: data.len(),
            headers: headers.into_iter().map(|s| s.to_string()).collect(),
            data,
        }
    }

    fn print_all(self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        writeln!(handle, "{}", self.headers.join(""))?;
        writeln!(handle, "-----------------------------------")?;

        for entry in self.data {
            writeln!(handle, "{}", entry)?;
        }

        Ok(())
    }

    async fn into_sheet(
        self,
        sheet_id: &str,
    ) -> Result<google_sheets::UpdateValuesResponse, google_sheets::APIError> {
        let sheets = match google_sheets::Sheets::initialize(&sheet_id).await {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let mut sheet_data: Vec<Vec<String>> = vec![self.headers];

        for d in self.data {
            sheet_data.push(d.into_sheet_entry());
        }

        sheets.refresh_entire_sheet(sheet_data).await
    }
}

#[throws]
#[tokio::main]
async fn main() {
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
            let gh_org = octocrab.orgs(&org);
            let repos: Vec<Repository> = all_repos(&gh_org).await?;
            let mut entries: Collection<ListEntry> = ListEntry::into_entry(vec![]);

            for repo in &repos {
                let count_prs = count_pull_requests(&octocrab, &org, &repo.name).await?;
                entries.data.push(ListEntry {
                    repository_name: repo.name.to_string(),
                    number_recent_pull_requests: count_prs,
                });
            }

            match gs {
                // if user specified a google sheet ID, they must want to export data to that sheet!
                Some(sheet_id) => match entries.into_sheet(&sheet_id).await {
                    Ok(res) => {
                        let sheet_url = if let Some(ss_id) = &res.spreadsheet_id {
                            google_sheets::Sheets::get_link_to_sheet(&ss_id)
                        } else {
                            String::from("unknown")
                        };
                        println!(
                            "Data successfully uploaded to your Google Sheet! {} \n\nLink to Sheet: {}",
                            res, sheet_url
                        );
                    }
                    Err(err) => println!(
                        "A problem occurred while uploading your data to Google Sheets! {:#?}",
                        err
                    ),
                },
                // user did not specify Sheet ID, so they must want to print to terminal
                None => entries.print_all()?,
            }
        }
    }
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
