use std::fs::File;
use std::io::Write;

use fehler::throws;
use stable_eyre::eyre::{Error, WrapErr};

use super::{Report, ReportConfig, ReportData};

impl Report {
    #[throws]
    pub(super) fn write_issue_closures(&self, _config: &ReportConfig, data: &ReportData) {
        let output = self.output_dir().join("issue-closures.csv");
        let output =
            &mut File::create(output).wrap_err("Failed to create file 'issue-closures.csv'")?;
        writeln!(output, "Organization,Repo,Opened,Closed,Delta,Time Period").unwrap();
        // TODO: collapse issue closures with the same org/repo into one row
        for (_, d) in &data.repo_infos.repos {
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
