use anyhow::Result;
use hf_hub::{
    api::{sync::ApiError, Siblings},
    Cache, Repo, RepoType,
};
use std::path::PathBuf;

const MODEL_EXTENSIONS: [&str; 2] = [".safetensors", ".json"];
const DATASET_EXTENSIONS: [&str; 1] = [".parquet"];

fn check_extensions(sibling: &Siblings, extensions: &[&'static str]) -> bool {
    match extensions.is_empty() {
        true => true,
        false => {
            for ext in extensions {
                if sibling.rfilename.ends_with(ext) {
                    return true;
                }
            }
            false
        }
    }
}

async fn download_repo_async(
    repo: Repo,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
    extensions: &[&'static str],
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
        .repo(repo);
    let siblings = api
        .info()
        .await?
        .siblings
        .into_iter()
        .filter(|x| check_extensions(x, extensions))
        .collect::<Vec<_>>();
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

pub async fn download_model_repo_async(
    repo_id: String,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    download_repo_async(
        match revision {
            Some(revision) => Repo::with_revision(repo_id, RepoType::Model, revision),
            None => Repo::model(repo_id),
        },
        cache,
        token,
        max_concurrent_downloads,
        progress_bar,
        &MODEL_EXTENSIONS,
    )
    .await
}

pub async fn download_dataset_repo_async(
    repo_id: String,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    download_repo_async(
        Repo::with_revision(
            repo_id,
            RepoType::Dataset,
            "refs/convert/parquet".to_owned(),
        ),
        cache,
        token,
        max_concurrent_downloads,
        progress_bar,
        &DATASET_EXTENSIONS,
    )
    .await
}

fn download_repo_sync(
    repo: Repo,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
    extensions: &[&'static str],
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
        .repo(repo);
    let res: Result<Vec<PathBuf>, ApiError> = api
        .info()?
        .siblings
        .into_iter()
        .filter(|x| check_extensions(x, extensions))
        .map(|x| api.get(&x.rfilename))
        .collect();
    Ok(res?)
}

pub fn download_model_repo_sync(
    repo_id: &str,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    download_repo_sync(
        match revision {
            Some(revision) => Repo::with_revision(repo_id.to_owned(), RepoType::Model, revision),
            None => Repo::model(repo_id.to_owned()),
        },
        cache,
        token,
        progress_bar,
        &MODEL_EXTENSIONS,
    )
}

pub fn download_dataset_repo_sync(
    repo_id: &str,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>> {
    download_repo_sync(
        Repo::with_revision(
            repo_id.to_owned(),
            RepoType::Dataset,
            "refs/convert/parquet".to_owned(),
        ),
        cache,
        token,
        progress_bar,
        &DATASET_EXTENSIONS,
    )
}
