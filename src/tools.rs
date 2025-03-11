use std::fs::File;
use std::io::{BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs, io};

use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::VecMap;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    deps: Vec<String>,
    #[serde(flatten)]
    install: Install,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Install {
    Download {
        version: String,
        url: String,
        ext: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        overrides: Vec<DownloadOverride>,
    },
    System {
        #[serde(default)]
        local: SystemLocal,
        #[serde(default, rename = "github-workflows")]
        github: SystemGithub,
    },
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct SystemLocal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct SystemGithub {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    install_action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct DownloadOverride {
    r#where: DownloadOverrideInfo,
    set: DownloadOverrideInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct DownloadOverrideInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    triple: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ext: Option<String>,
}

impl DownloadOverrideInfo {
    fn matches(&self, system: &SystemInfo) -> bool {
        let triple = self.triple.as_deref();
        triple.is_some_and(|triple| triple == system.triple)
            || self.os.as_deref().is_some_and(|os| os == system.os)
            || self.arch.as_deref().is_some_and(|arch| arch == system.arch)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Local {
    path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct GithubWorkflows {
    install_action: String,
}

#[derive(Debug, Clone, Copy)]
struct SystemInfo<'a> {
    triple: &'a str,
    arch: &'a str,
    os: &'a str,
}

impl<'a> SystemInfo<'a> {
    fn apply<'b: 'a>(&mut self, from: &'b DownloadOverrideInfo) {
        if let Some(triple) = from.triple.as_deref() {
            self.triple = triple;
        }
        if let Some(arch) = from.arch.as_deref() {
            self.arch = arch;
        }
        if let Some(os) = from.os.as_deref() {
            self.os = os;
        }
    }
}

static HOST: SystemInfo<'static> = SystemInfo {
    triple: include_str!(concat!(env!("OUT_DIR"), "/target")),
    arch: env::consts::ARCH,
    os: env::consts::OS,
};

pub struct DownloadManager<'a> {
    state: PathBuf,
    tools: &'a VecMap<Tool>,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("download failed: {0}")]
    Http(#[from] reqwest::Error),
}

impl<'a> DownloadManager<'a> {
    pub(crate) fn new(state: PathBuf, tools: &'a VecMap<Tool>) -> Self {
        Self { state, tools }
    }

    pub async fn run<P>(self, progress: P) -> Result<(), DownloadError>
    where
        P: DownloadProgress + Clone + Send + Sync + 'static,
        P::Bar: Send,
    {
        static USER_AGENT: &'static str =
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

        let root = Arc::new(self.state);
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap();

        let limit = 10;
        let semaphore = Arc::new(Semaphore::new(limit));
        let mut jobs = Vec::with_capacity(limit);
        for (name, tool) in self.tools.iter() {
            match &tool.install {
                Install::Download {
                    version,
                    url,
                    ext,
                    overrides,
                } => {
                    let mut ext = ext.as_str();
                    let mut system = HOST;
                    for override_info in overrides {
                        if override_info.r#where.matches(&system) {
                            system.apply(&override_info.set);
                            if let Some(ext_) = override_info.set.ext.as_deref() {
                                ext = ext_;
                            }
                        }
                    }

                    let version = version.clone();
                    let url = url
                        .replace("#version#", &version)
                        .replace("#triple#", &system.triple)
                        .replace("#arch#", &system.arch)
                        .replace("#os#", &system.os)
                        .replace("#ext#", ext);

                    let request = client.get(&url);

                    let semaphore = Arc::clone(&semaphore);
                    let root = Arc::clone(&root);
                    let name = name.to_owned();
                    let ext = ext.to_owned();
                    let progress = progress.clone();
                    jobs.push(tokio::spawn(async move {
                        let _permit = semaphore.acquire().await.unwrap();
                        download_and_install(&root, &name, &version, &ext, request, progress).await
                    }));
                }
                Install::System { .. } => {
                    // TODO
                }
            }
        }

        for job in jobs {
            // TODO: allow some to fail
            job.await.unwrap()?;
        }

        Ok(())
    }
}

pub trait DownloadProgress {
    type Bar: DownloadProgressBar;
    fn start(&self, name: &str, len: Option<u64>) -> Self::Bar;
}

pub trait DownloadProgressBar {
    fn update(&mut self, delta: u64);
    fn done(self);
}

async fn download_and_install<P: DownloadProgress>(
    root: &Path,
    tool: &str,
    version: &str,
    ext: &str,
    request: reqwest::RequestBuilder,
    progress: P,
) -> Result<(), DownloadError> {
    let install = root.join("tools").join(tool);
    fs::create_dir_all(&install)?;
    let install = install.join(version);
    if fs::exists(&install)? {
        return Ok(());
    }

    let downloads = root.join("downloads").join(tool);
    fs::create_dir_all(&downloads)?;

    let downloaded = downloads.join(format!("{version}.{ext}"));
    if !fs::exists(&downloaded)? {
        let mut downloaded = File::create(&downloaded)?;

        let response = request.send().await?;
        let mut progress = progress.start(&tool, response.content_length());
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded.write_all(&chunk)?;
            progress.update(chunk.len() as u64);
        }

        downloaded.flush()?;
        progress.done();
    };
    let downloaded = File::open(downloaded)?;

    // Extract into a temp directory that later gets moved into place to make things
    // a little less likely to end up half installed
    let temp = root.join("temp");
    fs::create_dir_all(&temp)?;
    let extract = tempfile::tempdir_in(temp)?;
    match ext {
        "tar.gz" => {
            let downloaded = BufReader::new(downloaded);
            let gzip = flate2::bufread::GzDecoder::new(downloaded);
            let mut tar = tar::Archive::new(gzip);
            tar.unpack(&extract)?;
        }
        _ => todo!(),
    }

    let entries = fs::read_dir(&extract)?.collect::<Result<Vec<_>, _>>()?;
    let mut to_install = extract.into_path();
    // Strip outermost directory
    if entries.len() == 1 {
        let path = entries[0].path();
        if path.is_dir() {
            to_install = path;
        }
    }

    fs::rename(to_install, install)?;

    Ok(())
}
