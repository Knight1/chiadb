use std::path::Path;

use super::first_changed_height;
use crate::{model::BlockActivity, rpc::BlockRecord, store::Store};

fn partially_collected_batch() -> (Store, Vec<BlockRecord>) {
    let store = Store::open(Path::new(":memory:")).unwrap();
    store.set_cursor(0).unwrap();
    let records: Vec<_> = (0..6)
        .map(|height| BlockRecord {
            height,
            header_hash: format!("block-{height}"),
            timestamp: None,
        })
        .collect();
    for height in [2, 5] {
        store
            .upsert_block(&BlockActivity {
                height,
                header_hash: records[height as usize].header_hash.clone(),
                day: None,
                additions: 0,
                removals: 0,
            })
            .unwrap();
    }
    (store, records)
}

#[test]
fn a_timed_out_batch_keeps_successful_out_of_order_responses() {
    let (store, records) = partially_collected_batch();

    assert_eq!(first_changed_height(&store, &records).unwrap(), None);
    assert_eq!(store.cursor().unwrap(), Some(0));
    assert_eq!(store.block_hash(2).unwrap().as_deref(), Some("block-2"));
    assert_eq!(store.block_hash(5).unwrap().as_deref(), Some("block-5"));
}

#[test]
fn a_real_hash_change_is_detected_after_an_uncollected_height() {
    let (store, mut records) = partially_collected_batch();
    records[5].header_hash = "replacement-5".to_owned();

    assert_eq!(first_changed_height(&store, &records).unwrap(), Some(5));
}
