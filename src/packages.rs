use deb822_lossless::{Deb822, FromDeb822, FromDeb822Paragraph, Paragraph, ParseError};
use reqwest::Client;
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use xz2::read::XzDecoder;

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
    DebControl(ParseControlError),
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

        let bytes = resp.bytes().await?.to_vec();
        let decompressed = if self.download_compress {
            let mut cursor = Cursor::new(&bytes);
            let mut decoder = XzDecoder::new(&mut cursor);
            let mut res = vec![];
            decoder.read_to_end(&mut res)?;
            res
        } else {
            bytes
        };

        f.write_all(&decompressed).await?;

        (decompressed.as_slice())
            .try_into()
            .map_err(FetchPackagesError::DebControl)
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

        let bytes = resp.bytes()?.to_vec();
        let decompressed = if self.download_compress {
            let mut cursor = Cursor::new(&bytes);
            let mut decoder = XzDecoder::new(&mut cursor);
            let mut res = vec![];
            decoder.read_to_end(&mut res)?;
            res
        } else {
            bytes
        };

        f.write_all(&decompressed)?;

        (decompressed.as_slice())
            .try_into()
            .map_err(FetchPackagesError::DebControl)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseControlError {
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Failed convert to package from paragraph")]
    Paragraph(String),
    #[error(transparent)]
    ParseError(#[from] ParseError),
}

pub struct Packages(pub Vec<Package>);

impl FromStr for Packages {
    type Err = ParseControlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pkgs: Deb822 = s.parse()?;
        let mut res = vec![];
        for para in pkgs.paragraphs() {
            let pkg =
                FromDeb822Paragraph::from_paragraph(&para).map_err(ParseControlError::Paragraph)?;
            res.push(pkg);
        }

        Ok(Self(res))
    }
}

impl TryFrom<&[u8]> for Packages {
    type Error = ParseControlError;

    fn try_from(input: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(input)?;
        let pkgs: Packages = s.parse()?;

        Ok(pkgs)
    }
}

impl FromStr for Package {
    type Err = ParseControlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pkg: Paragraph = s.parse()?;
        let pkg: Package =
            FromDeb822Paragraph::from_paragraph(&pkg).map_err(ParseControlError::Paragraph)?;

        Ok(pkg)
    }
}

impl TryFrom<&[u8]> for Package {
    type Error = ParseControlError;

    fn try_from(input: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(input)?;
        let pkgs: Package = s.parse()?;

        Ok(pkgs)
    }
}

#[derive(Debug, Clone, FromDeb822)]
pub struct Package {
    #[deb822(field = "Package")]
    pub package: String,
    #[deb822(field = "Architecture")]
    pub architecture: String,
    #[deb822(field = "Version")]
    pub version: String,
    #[deb822(field = "Section")]
    pub section: String,
    #[deb822(field = "Installed-Size")]
    pub install_size: u64,
    #[deb822(field = "Maintainer")]
    pub maintainer: String,
    #[deb822(field = "Filename")]
    pub filename: String,
    #[deb822(field = "Size")]
    pub size: u64,
    #[deb822(field = "SHA256")]
    pub sha256: String,
    #[deb822(field = "Description")]
    pub description: String,
    #[deb822(field = "Depends")]
    pub depends: Option<String>,
    #[deb822(field = "Provides")]
    pub provides: Option<String>,
    #[deb822(field = "Conflicts")]
    pub conflicts: Option<String>,
    #[deb822(field = "Replaces")]
    pub replaces: Option<String>,
    #[deb822(field = "Breaks")]
    pub breaks: Option<String>,
    #[deb822(field = "X-AOSC-Features")]
    pub featres: Option<String>,
}
