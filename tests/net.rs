//! Fetching from a real git host. Needs the network, so it is ignored by
//! default: `cargo test --test net -- --ignored`.
#![cfg(feature = "net")]

use vyrn::project::fetch::{tree_hash, unpack, Fetcher, HttpFetcher};
use vyrn::project::url::{select, ModuleUrl};

#[test]
#[ignore]
fn fetches_and_hashes_a_public_repository() {
    let url = ModuleUrl::parse("github.com/rust-lang/log@0.4").unwrap();
    let fetcher = HttpFetcher::new();
    let refs = fetcher.list_refs(&url).expect("list refs");
    let (version, commit) = select(&url.version, &refs).expect("a 0.4.x tag");
    assert!(
        version.trim_start_matches('v').starts_with("0.4."),
        "{version}"
    );
    let tree = unpack(&fetcher.archive(&url, &commit).expect("download")).expect("unpack");
    assert!(tree.contains_key("Cargo.toml"));
    // The same commit always hashes the same.
    let again = unpack(&fetcher.archive(&url, &commit).unwrap()).unwrap();
    assert_eq!(tree_hash(&tree), tree_hash(&again));
    println!(
        "{version} {commit} {} files {}",
        tree.len(),
        tree_hash(&tree)
    );
}
