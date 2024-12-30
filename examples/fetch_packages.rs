use std::fs::create_dir_all;

use libaosc::packages::FetchPackagesAsync;

#[tokio::main]
async fn main() {
    create_dir_all("./test").unwrap();
    let fetch = FetchPackagesAsync::new(true, "./test", None);
    let pkgs = fetch.fetch_packages("amd64", "stable").await.unwrap();
    dbg!(pkgs.0.first());
}
