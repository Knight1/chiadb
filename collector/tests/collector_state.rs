use std::{env, fs, path::PathBuf};

use chia_db_collector::store::Store;

fn test_directory(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "chia-db-collector-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn persists_initial_full_node_contact() {
    let directory = test_directory("network-contact");
    let state_path = directory.join("state.sqlite");
    let store = Store::open(&state_path).unwrap();

    assert!(!store.initial_network_contact_observed().unwrap());
    store.set_initial_network_contact_observed(true).unwrap();
    assert!(store.initial_network_contact_observed().unwrap());

    drop(store);
    let reopened = Store::open(&state_path).unwrap();
    assert!(reopened.initial_network_contact_observed().unwrap());

    drop(reopened);
    fs::remove_dir_all(directory).unwrap();
}
