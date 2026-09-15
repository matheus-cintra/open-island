use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{self, Read};

pub const PAGE_BYTES: usize = 32768;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotPage {
    pub snapshot_id: u64,
    pub page_index: u64,
    pub bytes: Vec<u8>,
    pub end: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
}

pub fn decode<T: DeserializeOwned>(
    snapshot_id: u64,
    next: impl FnMut(u64) -> Result<SnapshotPage, String>,
) -> Result<T, String> {
    let reader = PageReader {
        snapshot_id,
        next,
        page_index: 0,
        bytes: Vec::new(),
        offset: 0,
        received: 0,
        end: false,
    };
    let mut decoder = serde_json::Deserializer::from_reader(reader);
    let value = T::deserialize(&mut decoder).map_err(|e| format!("snapshot_parse: {e}"))?;
    decoder.end().map_err(|e| format!("snapshot_parse: {e}"))?;
    Ok(value)
}

struct PageReader<F> {
    snapshot_id: u64,
    next: F,
    page_index: u64,
    bytes: Vec<u8>,
    offset: usize,
    received: u64,
    end: bool,
}
impl<F: FnMut(u64) -> Result<SnapshotPage, String>> Read for PageReader<F> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.offset == self.bytes.len() && !self.end {
            let page = (self.next)(self.page_index).map_err(io::Error::other)?;
            if page.snapshot_id != self.snapshot_id
                || page.page_index != self.page_index
                || page.bytes.len() > PAGE_BYTES
                || (!page.end && (page.bytes.is_empty() || page.total_bytes.is_some()))
            {
                return Err(io::Error::other("invalid_snapshot_page"));
            }
            self.received = self
                .received
                .checked_add(page.bytes.len() as u64)
                .ok_or_else(|| io::Error::other("snapshot_size_overflow"))?;
            if page.end && page.total_bytes != Some(self.received) {
                return Err(io::Error::other("invalid_snapshot_total"));
            }
            self.page_index = self
                .page_index
                .checked_add(1)
                .ok_or_else(|| io::Error::other("snapshot_index_overflow"))?;
            self.bytes = page.bytes;
            self.offset = 0;
            self.end = page.end;
        }
        let count = output.len().min(self.bytes.len() - self.offset);
        output[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}
