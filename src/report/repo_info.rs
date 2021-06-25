use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RepoInfo {
    #[serde(rename = "#")]
    pub row: usize,
    #[serde(rename = "Repository Name")]
    pub repo: String,
    #[serde(rename = "# of PRs")]
    pub num_prs: u64,
}
