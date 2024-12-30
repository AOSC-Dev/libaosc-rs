use std::fs::create_dir_all;

use libaosc::packages::FetchPackages;

fn main() {
    create_dir_all("./test").unwrap();

    let fetch = FetchPackages::new(true, "./test", None);
    let pkgs = fetch.fetch_packages("amd64", "stable").unwrap();
    dbg!(pkgs.0.first());
}
