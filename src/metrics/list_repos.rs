use async_trait::async_trait;
use fehler::throws;
use log::debug;
use stable_eyre::eyre::{self, Error};
use tokio::sync::mpsc::Sender;
use toml::value::Datetime;

use super::{util, Graphql, Producer};

#[derive(Debug)]
pub struct ListReposForOrg {
    graphql: Graphql,
    org_name: String,
    repo_names: Vec<String>,
    start_date: Datetime,
    end_date: Datetime,
}

impl ListReposForOrg {
    pub fn new(
        graphql: Graphql,
        org_name: String,
        repo_names: Vec<String>,
        start_date: Datetime,
        end_date: Datetime,
    ) -> Self {
        ListReposForOrg {
            graphql,
            org_name,
            repo_names,
            start_date,
            end_date,
        }
    }
}

impl ListReposForOrg {
    fn to_repo(&self, repo_name: &str) -> Repo {
        Repo {
            graphql: self.graphql.clone(),
            org_name: self.org_name.clone(),
            repo_name: repo_name.to_string(),
            start_date: self.start_date.clone(),
            end_date: self.end_date.clone(),
        }
    }
}

#[async_trait]
impl Producer for ListReposForOrg {
    fn column_names(&self) -> Vec<String> {
        vec![
            String::from("Organization"),
            String::from("Repository"),
            String::from("PRs Opened"),
            String::from("Issues Opened"),
            String::from("Issues Closed"),
            String::from("Start Date"),
            String::from("End Date"),
        ]
    }

    async fn producer_task(mut self, tx: Sender<Vec<String>>) -> Result<(), eyre::Error> {
        for repo_name in &self.repo_names {
            let mut repo = self.to_repo(repo_name);
            let count_prs = repo.count_pulls().await?;
            let count_issues = repo.spawn_count_issue_closures().await?;

            tx.send(vec![
                self.org_name.clone(),
                repo_name.to_owned(),
                count_prs.to_string(),
                count_issues.opened.to_string(),
                count_issues.closed.to_string(),
                self.start_date.to_string(),
                self.end_date.to_string(),
            ])
            .await?;
        }

        Ok(())
    }
}

#[derive(Default, Debug)]
struct IssueClosuresCount {
    opened: usize,
    closed: usize,
}

#[derive(Clone, Debug)]
struct Repo {
    graphql: Graphql,
    org_name: String,
    repo_name: String,
    start_date: Datetime,
    end_date: Datetime,
}

impl Repo {
    #[throws]
    async fn spawn_count_issue_closures(&self) -> IssueClosuresCount {
        let mut repo = self.clone();
        let mut clone = self.clone();

        let futures = vec![repo.count_issues("created"), clone.count_issues("closed")];

        let resolved = futures::future::try_join_all(futures).await?;

        let result = IssueClosuresCount {
            opened: resolved[0],
            closed: resolved[1],
        };

        debug!(
            "Retried issue closure info for {}/{}: {:?}",
            self.org_name, self.repo_name, result
        );

        result
    }

    #[inline]
    #[throws]
    async fn count_pulls(&mut self) -> usize {
        util::count_pull_requests(
            &mut self.graphql,
            &self.org_name,
            &self.repo_name,
            &self.start_date,
            &self.end_date,
        )
        .await?
    }

    #[inline]
    #[throws]
    async fn count_issues(&mut self, state: &str) -> usize {
        util::count_issues(
            &mut self.graphql,
            &self.org_name,
            &self.repo_name,
            &self.start_date,
            &self.end_date,
            state,
        )
        .await?
    }
}
