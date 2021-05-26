use anyhow::Error;
use fehler::throws;

/// Finds the token in the user's environment, panicking if no suitable token
/// can be found.
#[throws]
pub fn github_token() -> String {
    if let Some(s) = get_token_from_env() {
        return s;
    }

    if let Some(s) = get_token_from_git_config()? {
        return s;
    }

    anyhow::bail!("could not find github token");
}

fn get_token_from_env() -> Option<String> {
    match std::env::var("GITHUB_TOKEN") {
        Ok(v) => Some(v),
        Err(_) => None,
    }
}

#[throws]
fn get_token_from_git_config() -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("config")
        .arg("--get")
        .arg("github.oauth-token")
        .output()?;
    if output.status.success() {
        let git_token = String::from_utf8(output.stdout)?.trim().to_string();
        Some(git_token)
    } else {
        None
    }
}
