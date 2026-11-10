use chia_db_collector::rpc::{BlockRecord, validate_block_range};

#[test]
fn accepts_contiguous_block_records() {
    let records = vec![
        BlockRecord {
            height: 10,
            header_hash: "0x10".to_owned(),
            timestamp: None,
        },
        BlockRecord {
            height: 11,
            header_hash: "0x11".to_owned(),
            timestamp: None,
        },
    ];

    assert!(validate_block_range(&records, 10, 12).is_ok());
}

#[test]
fn rejects_missing_block_records() {
    let records = vec![BlockRecord {
        height: 10,
        header_hash: "0x10".to_owned(),
        timestamp: None,
    }];

    assert!(validate_block_range(&records, 10, 12).is_err());
}
