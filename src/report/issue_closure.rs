use std::fs::File;
use std::io::Write;
use std::path::Path;

use fehler::throws;
use log::debug;
use serde::{Deserialize, Serialize};
use stable_eyre::eyre::{Error, WrapErr};

use super::{Report, ReportConfig, ReportData};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct IssueClosure {
    #[serde(rename = "Organization")]
    org: String,
    #[serde(rename = "Repository")]
    repo: String,
    #[serde(rename = "Issues Opened")]
    num_opened: u64,
    #[serde(rename = "Issues Closed")]
    num_closed: u64,
    #[serde(rename = "Start Date")]
    start: String,
    #[serde(rename = "End Date")]
    end: String,
}

impl IssueClosure {
    #[throws]
    pub(super) async fn parse_csv(path: &Path) -> Vec<Self> {
        let mut rdr = csv::Reader::from_path(path)
            .wrap_err_with(|| format!("Failed to create reader from path {:?}", path))?;
        let mut vec = Vec::new();
        for result in rdr.deserialize() {
            let record: IssueClosure =
                result.wrap_err("Failed to deserialize while parsing issue closure")?;
            vec.push(record);
        }
        vec
    }
}

impl Report {
    #[throws]
    pub(super) fn write_issue_closures(&self, _config: &ReportConfig, data: &ReportData) {
        let output = self.output_dir().join("issue-closures.csv");
        let output = &mut File::create(output)?;
        writeln!(output, "Organization,Repo,Opened,Closed,Delta,Time Period").unwrap();
        // TODO: collapse issue closures with the same org/repo into one row
        for d in &data.issue_closures {
            writeln!(
                output,
                "{},{},{},{},{},{}",
                d.org,
                d.repo,
                d.num_opened,
                d.num_closed,
                (d.num_opened as i64 - d.num_closed as i64),
                format!("{}<>{}", d.start, d.end)
            )
            .unwrap();
        }
    }

    #[throws]
    pub(super) async fn issue_closures(&self, _config: &ReportConfig) -> Vec<IssueClosure> {
        debug!("Finding issue closures...");
        // path to relevant input data
        let repo_info = self.input_dir().join("repo-infos.csv");

        // `IssueClosure` does not need to 'produce' here, since the necessary
        // data for this report is generated with `metrics::ListReposForOrg`.
        // Thus, no need to call `self.produce_input`
        //
        // See: `Report::repo_infos` method in `src/report/repo_info.rs`

        IssueClosure::parse_csv(&repo_info.clone())
            .await
            .wrap_err("Failed to parse issue closure information")?
    }
}
