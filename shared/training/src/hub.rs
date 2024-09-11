use anyhow::Result;
use hf_hub::{api::sync::ApiError, Cache, Repo, RepoType};
use std::path::PathBuf;

pub async fn download_repo(
    repo_id: String,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    let builder = hf_hub::api::tokio::ApiBuilder::new();
    let cache = match cache {
        Some(cache) => Cache::new(cache),
        None => Cache::default(),
    };
    let api = builder
        .with_cache_dir(cache.path().clone())
        .with_token(token.or(cache.token()))
        .with_progress(progress_bar)
        .build()?
        .repo(match revision {
            Some(revision) => Repo::with_revision(repo_id, RepoType::Model, revision),
            None => Repo::model(repo_id),
        });
    let siblings = api.info().await?.siblings;
    let mut ret: Vec<PathBuf> = Vec::new();
    for chunk in siblings.chunks(max_concurrent_downloads.unwrap_or(siblings.len())) {
        let futures = chunk
            .iter()
            .map(|x| api.get(&x.rfilename))
            .collect::<Vec<_>>();
        for future in futures {
            ret.push(future.await?);
        }
    }
    Ok(ret)
}

pub fn download_repo_sync(
    repo_id: String,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    let builder = hf_hub::api::sync::ApiBuilder::new();
    let cache = match cache {
        Some(cache) => Cache::new(cache),
        None => Cache::default(),
    };
    let api = builder
        .with_cache_dir(cache.path().clone())
        .with_token(token.or(cache.token()))
        .with_progress(progress_bar)
        .build()?
        .repo(match revision {
            Some(revision) => Repo::with_revision(repo_id, RepoType::Model, revision),
            None => Repo::model(repo_id),
        });
    let res: Result<Vec<PathBuf>, ApiError> = api
        .info()?
        .siblings
        .into_iter()
        .map(|x| api.get(&x.rfilename))
        .collect();
    Ok(res?)
}
