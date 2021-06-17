use anyhow::Error;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fehler::throws;
use octocrab::models::Repository;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::google_sheets::Sheets;
use crate::util;

#[async_trait]
pub trait Producer {
    fn column_names(&self) -> Vec<String>;
    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String>;
}

#[async_trait]
pub trait Consumer {
    async fn consume(&self, rx: &mut Receiver<Vec<String>>, column_names: Vec<String>);
}

pub struct ListReposForOrg {
    org_name: String,
}

impl ListReposForOrg {
    pub fn new(org_name: &str) -> Self {
        ListReposForOrg {
            org_name: String::from(org_name),
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![String::from("Repository Name"), String::from("# of PRs")]
    }

    async fn producer_task(self, tx: Sender<Vec<String>>) -> Result<(), String> {
        let octo = octocrab::instance();
        let gh_org = octo.orgs(&self.org_name);

        let repos: Vec<Repository> = match all_repos(&gh_org).await {
            Ok(r) => r,
            Err(e) => {
                return Err(format!(
                    "Ran into an error while gathering repositories! {}",
                    e
                ));
            }
        };

        for repo in &repos {
            match count_pull_requests(&self.org_name, &repo.name).await {
                Ok(count_prs) => {
                    if let Err(e) = tx
                        .send(vec![String::from(&repo.name), count_prs.to_string()])
                        .await
                    {
                        return Err(format!("{:#?}", e));
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "Ran into an issue while counting PRs for repository {}: {}",
                        &repo.name, e
                    ));
                }
            }
        }

        Ok(())
    }
}

pub struct ExportToSheets {
    sheet_id: String,
}

impl ExportToSheets {
    pub fn new(sheet_id: &str) -> Self {
        ExportToSheets {
            sheet_id: String::from(sheet_id),
        }
    }
}

#[async_trait]
impl Consumer for ExportToSheets {
    async fn consume(&self, rx: &mut Receiver<Vec<String>>, column_names: Vec<String>) {
        let sheets = match Sheets::initialize(&self.sheet_id).await {
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
        if let Err(e) = sheets.append(column_names).await {
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
}

pub struct Print;

#[async_trait]
impl Consumer for Print {
    async fn consume(&self, rx: &mut Receiver<Vec<String>>, column_names: Vec<String>) {
        println!(
            "{}\t{}\n------------------------",
            column_names[1], column_names[0]
        );
        while let Some(entry) = rx.recv().await {
            println!("{}\t\t{}", &entry[1], &entry[0]);
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
async fn count_pull_requests(org_name: &String, repo_name: &str) -> usize {
    let octo = octocrab::instance();
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
