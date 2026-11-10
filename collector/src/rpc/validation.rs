use anyhow::{Context, Result, bail};

use super::BlockRecord;

pub fn validate_block_range(records: &[BlockRecord], start: u32, end: u32) -> Result<()> {
    let expected_count = usize::try_from(end - start).context("block range exceeds usize")?;
    if records.len() != expected_count {
        bail!(
            "get_block_records returned {} records for expected range {start}..{end}",
            records.len()
        );
    }
    for (offset, record) in records.iter().enumerate() {
        let expected = start
            .checked_add(u32::try_from(offset).context("record offset exceeds u32")?)
            .context("block height overflow")?;
        if record.height != expected {
            bail!(
                "get_block_records was not contiguous: expected height {expected}, got {}",
                record.height
            );
        }
    }
    Ok(())
}
