use super::{
    repo_info::RepoInfo, repo_participant::RepoParticipant, Report, ReportConfig, ReportData,
};
use fehler::throws;
use serde::Serialize;
use stable_eyre::eyre::{Error, WrapErr};
use std::fs::File;

#[derive(Debug, Serialize)]
struct HighContributorRow {
    repo: String,
    number_of_prs: u64,
    total_participants: u64,
    total_authors: u64,
    total_reviewers: u64,
    top_author: String,
    top_author_percentage: u64,
    top_reviewer: String,
    top_reviewer_percentage: u64,
    top_participant: String,
    top_participant_percentage: u64,
    saturation_authors: u64,
    saturation_author_names: String,
    saturation_reviewers: u64,
    saturation_reviewer_names: String,
    high_contributors: u64,
    high_contributor_names: String,
}

impl Report {
    #[throws]
    pub(super) fn write_high_contributors(&self, config: &ReportConfig, data: &ReportData) {
        let high_contributor_rows = self.high_contributor_rows(&config, &data);
        let output = self.output_dir().join("high-contributors.csv");
        write_high_contributor_rows(
            &mut File::create(output.clone())
                .wrap_err_with(|| format!("Failed to create output file {:?}", output))?,
            &high_contributor_rows,
        )?;
    }

    fn high_contributor_rows(
        &self,
        config: &ReportConfig,
        data: &ReportData,
    ) -> Vec<HighContributorRow> {
        config
            .github
            .repos
            .iter()
            .map(|repo| self.high_contributor_row(config, data, repo))
            .collect()
    }

    fn high_contributor_row(
        &self,
        config: &ReportConfig,
        data: &ReportData,
        repo: &str,
    ) -> HighContributorRow {
        let repo_info = &data.repo_infos.get(repo);

        let (top_author, top_author_percentage) = data
            .repo_participants
            .top_participant(repo_info, |p| p.authored);

        let (top_reviewer, top_reviewer_percentage) = data
            .repo_participants
            .top_participant(repo_info, |p| p.reviewed_or_resolved());

        let (saturation_reviewer_names, saturation_reviewers) = self.saturation(
            data,
            config.high_contributor.reviewer_saturation_threshold,
            repo_info,
            |p| p.reviewed_or_resolved(),
        );

        let (saturation_author_names, saturation_authors) = self.saturation(
            data,
            config.high_contributor.author_saturation_threshold,
            repo_info,
            |p| p.authored,
        );

        let (top_participant, top_participant_percentage) = data
            .repo_participants
            .top_participant(repo_info, |p| p.participated_in);

        let total_authors = data
            .repo_participants
            .in_repo(repo_info)
            .filter(|p| p.authored > 0)
            .count() as u64;
        let total_participants = data
            .repo_participants
            .in_repo(repo_info)
            .filter(|p| p.participated_in > 0)
            .count() as u64;
        let total_reviewers = data
            .repo_participants
            .in_repo(repo_info)
            .filter(|p| p.reviewed_or_resolved() > 0)
            .count() as u64;

        let high_contributors: Vec<&RepoParticipant> = data
            .repo_participants
            .in_repo(repo_info)
            .filter(|p| repo_info.is_high_contributor(config, p))
            .collect();

        HighContributorRow {
            repo: repo.to_string(),
            number_of_prs: repo_info.num_prs,
            total_authors,
            total_participants,
            total_reviewers,
            top_author,
            top_author_percentage,
            top_reviewer,
            top_reviewer_percentage,
            top_participant,
            top_participant_percentage,
            saturation_reviewer_names,
            saturation_reviewers,
            saturation_author_names,
            saturation_authors,
            high_contributors: high_contributors.len() as u64,
            high_contributor_names: high_contributors
                .iter()
                .map(|p| p.participant.to_string())
                .collect::<Vec<_>>()
                .join(","),
        }
    }

    /// Computes the number of participants needed to reach saturation_threshold_percentage% of total PRs.
    ///
    /// Returns a string with their names and the number of participants.
    fn saturation(
        &self,
        data: &ReportData,
        saturation_threshold_percentage: u64,
        repo_info: &RepoInfo,
        key: impl Fn(&RepoParticipant) -> u64,
    ) -> (String, u64) {
        let mut participants: Vec<(u64, &String)> = data
            .repo_participants
            .in_repo(repo_info)
            .map(|p| (key(p), &p.participant))
            .collect();
        participants.sort();
        participants.reverse();

        let mut running_total = 0;
        let target = repo_info.num_prs * saturation_threshold_percentage / 100;
        let mut output = vec![];
        for (count, participant) in participants {
            running_total += count;
            let percent = crate::util::percentage(count, repo_info.num_prs);
            output.push(format!("{} ({}%)", participant, percent));
            if running_total > target {
                break;
            }
        }

        (output.join(", "), output.len() as u64)
    }
}

#[throws]
fn write_high_contributor_rows(
    out: &mut impl std::io::Write,
    high_contributor_rows: &[HighContributorRow],
) {
    let mut csv = csv::Writer::from_writer(out);
    for row in high_contributor_rows {
        csv.serialize(row).wrap_err_with(|| {
            format!(
                "Failed to serialize row while writing high contributors: {:?}",
                row
            )
        })?;
    }
}
