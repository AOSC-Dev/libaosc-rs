use std::path::{Path, PathBuf};

use async_compression::tokio::write::XzDecoder;
use oma_debcontrol::Field;
use reqwest::Client;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

const USER_AGENT: &str = "aosc";
const DEFAULT_MIRROR: &str = "https://repo.aosc.io/debs";

#[cfg(feature = "async")]
pub struct FetchPackagesAsync {
    download_compress: bool,
    client: Client,
    download_to: PathBuf,
    mirror_url: String,
}

#[derive(Debug, Error)]
pub enum FetchPackagesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Failed to parse string to deb822 format")]
    DebControl,
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
}

#[cfg(feature = "async")]
impl FetchPackagesAsync {
    pub fn new<P: AsRef<Path>>(
        download_compress: bool,
        download_to: P,
        mirror_url: Option<&str>,
    ) -> Self {
        Self {
            download_compress,
            client: Client::builder().user_agent(USER_AGENT).build().unwrap(),
            download_to: download_to.as_ref().to_path_buf(),
            mirror_url: mirror_url.unwrap_or(DEFAULT_MIRROR).to_string(),
        }
    }

    pub async fn fetch_packages(
        &self,
        arch: &str,
        branch: &str,
    ) -> Result<Packages, FetchPackagesError> {
        let download_url = format!(
            "{}/dists/{branch}/main/binary-{arch}/Packages{}",
            self.mirror_url,
            if self.download_compress { ".xz" } else { "" }
        );

        let resp = self
            .client
            .get(download_url)
            .send()
            .await?
            .error_for_status()?;

        let dir = &self.download_to;

        if !dir.exists() {
            tokio::fs::create_dir_all(dir).await?;
        }

        let mut f = tokio::fs::File::create(dir.join("Packages")).await?;

        let mut writer: Box<dyn AsyncWrite + Unpin + Send> = if self.download_compress {
            Box::new(XzDecoder::new(&mut f))
        } else {
            Box::new(&mut f)
        };

        let bytes = resp.bytes().await?.to_vec();
        writer.write_all(&bytes).await?;

        Ok(Packages::from_bytes_async(bytes).await?)
    }
}

#[cfg(feature = "blocking")]
pub struct FetchPackages {
    download_compress: bool,
    client: reqwest::blocking::Client,
    download_to: PathBuf,
    mirror_url: String,
}

#[cfg(feature = "blocking")]
impl FetchPackages {
    pub fn new<P: AsRef<Path>>(
        download_compress: bool,
        download_to: P,
        mirror_url: Option<&str>,
    ) -> Self {
        Self {
            download_compress,
            client: reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
            download_to: download_to.as_ref().to_path_buf(),
            mirror_url: mirror_url.unwrap_or(DEFAULT_MIRROR).to_string(),
        }
    }

    pub fn fetch_packages(&self, arch: &str, branch: &str) -> Result<Packages, FetchPackagesError> {
        let download_url = format!(
            "{}/dists/{branch}/main/binary-{arch}/Packages{}",
            self.mirror_url,
            if self.download_compress { ".xz" } else { "" }
        );

        let resp = self.client.get(download_url).send()?.error_for_status()?;

        let dir = &self.download_to;

        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }

        let mut f = std::fs::File::create(dir.join("Packages"))?;

        let mut writer: Box<dyn std::io::Write + Unpin + Send> = if self.download_compress {
            Box::new(xz2::write::XzDecoder::new(&mut f))
        } else {
            Box::new(&mut f)
        };

        let bytes = resp.bytes()?.to_vec();
        writer.write_all(&bytes)?;

        Ok(Packages::from_bytes(&bytes)?)
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub package: String,
    pub architecture: String,
    pub version: String,
    pub section: String,
    pub install_size: u64,
    pub maintainer: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub description: String,
    pub depends: Option<String>,
    pub provides: Option<String>,
    pub conflicts: Option<String>,
    pub replaces: Option<String>,
    pub breaks: Option<String>,
}

pub struct Packages(Vec<Package>);

impl Packages {
    #[cfg(feature = "async")]
    pub async fn from_bytes_async(bytes: Vec<u8>) -> Result<Self, FetchPackagesError> {
        let res = tokio::task::spawn_blocking(move || get_packages(&bytes)).await??;

        Ok(Packages(res))
    }

    #[cfg(feature = "blocking")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FetchPackagesError> {
        let packages = get_packages(bytes)?;

        Ok(Packages(packages))
    }

    pub fn get_packages(&self) -> &Vec<Package> {
        &self.0
    }
}

fn get_packages(bytes: &[u8]) -> Result<Vec<Package>, FetchPackagesError> {
    let input = std::str::from_utf8(bytes)?;
    let mut res = vec![];
    let parse_release =
        oma_debcontrol::parse_str(input).map_err(|_| FetchPackagesError::DebControl)?;

    for i in parse_release {
        let fields = i.fields;
        let package = find_value(&fields, "Package").unwrap_or_default();
        let arch = find_value(&fields, "Architecture").unwrap_or_default();
        let version = find_value(&fields, "Version").unwrap_or_default();
        let section = find_value(&fields, "Section").unwrap_or_default();
        let install_size = find_value(&fields, "Install-Size")
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);

        let maintainer = find_value(&fields, "Maintainer").unwrap_or_default();
        let filename = find_value(&fields, "Filename").unwrap_or_default();
        let size = find_value(&fields, "Size")
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);

        let sha256 = find_value(&fields, "SHA256").unwrap_or_default();
        let description = find_value(&fields, "Description").unwrap_or_default();
        let depends = find_value(&fields, "Depends");
        let provides = find_value(&fields, "Provides");
        let conflicts = find_value(&fields, "Conflicts");
        let replaces = find_value(&fields, "Replaces");
        let breaks = find_value(&fields, "Breaks");

        res.push(Package {
            package,
            architecture: arch,
            version,
            section,
            install_size,
            maintainer,
            filename,
            size,
            sha256,
            description,
            depends,
            provides,
            conflicts,
            replaces,
            breaks,
        });
    }
    Ok(res)
}

fn find_value<'a>(fields: &'a [Field<'a>], key: &'a str) -> Option<String> {
    fields
        .into_iter()
        .find(|x| x.name == key)
        .map(|x| x.value.to_string())
}
