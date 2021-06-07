use fehler::throws;
use octocrab::Page;
use std::future::Future;

#[throws(octocrab::Error)]
pub async fn accumulate_pages<T, F>(mut data: impl FnMut(u32) -> F) -> Vec<T>
where
    F: Future<Output = Result<Page<T>, octocrab::Error>>,
{
    let mut repos = vec![];
    for page in 1_u32.. {
        let page = data(page).await?;
        let items_len = page.items.len();
        repos.extend(page.items);
        if items_len == 0 {
            break;
        }
    }
    return repos;
}
