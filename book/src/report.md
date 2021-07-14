# Generating a report

Optopodi is geared to generate **reports**. These are CSV files that summarize important information about your repository or repositories. To create a report you need to take the following steps:

- Make a directory `$DIR` for the report. We recommend a name like `data/2021-06-26`.
- Create a `report.toml` file in that directory. You can start with [the template](https://github.com/optopodi/optopodi/blob/main/report-template.toml) and customize it.
- Configure a github token. This is loaded from one of two sources:
  - The `GITHUB_TOKEN` environment variable, if present.
  - Otherwise, from the `github.oauth-token` setting in `~/.gitconfig`.
- Optionally, create a `crate-information.json` file in `$DIR`.
  - This defines notable crates from the ecosystem that you wish to analyze.
  - If you don't have such a file, it will be generated for you. However, if the file is present, Optopodi will make use of the existing `crate-information.json` for reproducibility.
  - You can generate this file using the [top-crates](https://github.com/integer32llc/rust-playground/tree/master/top-crates) crate from the Rust playground if you wish to produce it manually.
- Run `cargo run -- report $DIR`. This will populate the following subdirectories:
  - `$DIR/graphql` -- saved results of graphql queries. These can be "replayed" later to avoid hitting the network. This makes things faster and avoids generating tons of github API calls when debugging (which can easily exceed your quota).
  - `$DIR/inputs` -- intermediate CSV files containing data extracted from the graphql queries.
  - `$DIR/outputs` -- contains the CSV files you are meant to look at.
  - `$DIR/crate-information.json` will be generated if absent. This defines notable crates from the ecosystem that you wish to analyze.
- You can run with the `--replay-graphql` setting to re-use saved graphql queries: `cargo run -- --replay-graphql report $DIR`
  - This is most useful when debugging or tweaking the code.
