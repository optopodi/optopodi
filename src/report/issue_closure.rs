use fehler::throws;
use serde::{Deserialize, Serialize};
use stable_eyre::eyre::{Error, WrapErr};

use super::{Report, ReportConfig, ReportData};

use std::fs::File;
use std::path::Path;

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
        use std::io::Write;
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
}
