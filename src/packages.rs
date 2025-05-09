use deb822_lossless::{Deb822, FromDeb822, FromDeb822Paragraph, Paragraph, ParseError};

#[cfg(feature = "download")]
use std::io::{self, ErrorKind, Read, Write};

#[cfg(feature = "download")]
use std::path::{Path, PathBuf};

use std::str::FromStr;
use thiserror::Error;

#[cfg(feature = "download")]
const USER_AGENT: &str = "oma/1.14.514";

#[cfg(feature = "download")]
const DEFAULT_MIRROR: &str = "https://repo.aosc.io/debs";

#[cfg(feature = "async")]
pub struct FetchPackagesAsync {
    download_compress: bool,
    client: reqwest::Client,
    download_to: PathBuf,
    mirror_url: String,
}

#[derive(Debug, Error)]
pub enum FetchPackagesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[cfg(feature = "download")]
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("Failed to parse string to deb822 format")]
    DebControl(ParseControlError),
    #[cfg(feature = "async")]
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
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
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

        let bytes_stream = futures::TryStreamExt::into_async_read(futures::TryStreamExt::map_err(
            resp.bytes_stream(),
            |e| io::Error::new(ErrorKind::Other, e),
        ));

        let reader: &mut (dyn futures::AsyncRead + Unpin + Send) = if self.download_compress {
            &mut async_compression::futures::bufread::XzDecoder::new(futures::io::BufReader::new(
                bytes_stream,
            ))
        } else {
            &mut futures::io::BufReader::new(bytes_stream)
        };

        let mut reader = tokio_util::compat::FuturesAsyncReadCompatExt::compat(reader);

        let dir = &self.download_to;

        if !dir.exists() {
            tokio::fs::create_dir_all(dir).await?;
        }

        let mut f = tokio::fs::File::create(dir.join("Packages")).await?;
        let mut buf = vec![];
        tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf).await?;
        tokio::io::AsyncWriteExt::write_all(&mut f, &buf).await?;

        (buf.as_slice())
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

        let mut resp = self.client.get(download_url).send()?.error_for_status()?;

        let dir = &self.download_to;

        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }

        let mut f = std::fs::File::create(dir.join("Packages"))?;

        let mut reader: Box<dyn Read> = if self.download_compress {
            Box::new(liblzma::read::XzDecoder::new(&mut resp))
        } else {
            Box::new(resp)
        };

        let mut res = vec![];
        reader.read_to_end(&mut res)?;

        f.write_all(&res)?;

        (res.as_slice())
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
