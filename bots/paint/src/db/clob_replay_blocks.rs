use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, bail};

use crate::types::{FeedEvent, ReplayFidelity};

/// Current binary payload schema for `clob_replay_blocks`.
pub const CLOB_REPLAY_BLOCK_SCHEMA_VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"BCRB0001";
const NONE_U32: u32 = u32::MAX;
const NONE_U64: u64 = u64::MAX;

/// One decoded CLOB replay row from a compressed block.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedClobReplayEvent {
    pub received_at_ms: u64,
    pub event_at_ms: u64,
    pub received_at_us: Option<u64>,
    pub event_at_us: Option<u64>,
    pub source: String,
    pub event_type: String,
    pub source_topic: Option<String>,
    pub connection_id: Option<String>,
    pub sequence_key: Option<String>,
    pub market_id: Option<String>,
    pub asset_id: Option<String>,
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_size: f64,
    pub ask_size: f64,
    pub microprice: Option<f64>,
    pub fidelity: ReplayFidelity,
}

/// Encoded block ready for `SQLite` persistence.
#[derive(Debug, Clone)]
pub struct EncodedClobReplayBlock {
    pub min_received_at_ms: u64,
    pub max_received_at_ms: u64,
    pub min_received_at_us: Option<u64>,
    pub max_received_at_us: Option<u64>,
    pub row_count: u32,
    pub up_rows: u32,
    pub down_rows: u32,
    pub schema_version: u32,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub checksum: String,
    pub payload: Vec<u8>,
}

/// Encode one CLOB block with local dictionaries and zstd compression.
pub fn encode_events(
    events: &[FeedEvent],
    zstd_level: i32,
) -> anyhow::Result<EncodedClobReplayBlock> {
    if events.is_empty() {
        bail!("cannot encode empty CLOB replay block");
    }
    let dictionaries = Dictionaries::from_events(events);
    let mut payload = Vec::new();
    payload.extend_from_slice(MAGIC);
    write_u32(&mut payload, CLOB_REPLAY_BLOCK_SCHEMA_VERSION);
    write_dictionaries(&mut payload, &dictionaries)?;
    write_u32(&mut payload, u32_len(events.len(), "row count")?);
    let mut up_rows = 0_u32;
    let mut down_rows = 0_u32;
    let mut millis_floor = u64::MAX;
    let mut millis_ceiling = 0_u64;
    let mut microsecond_floor: Option<u64> = None;
    let mut microsecond_ceiling: Option<u64> = None;
    for event in events {
        let side_code = side_code(&event.source)?;
        if side_code == 1 {
            up_rows = up_rows.saturating_add(1);
        } else {
            down_rows = down_rows.saturating_add(1);
        }
        millis_floor = millis_floor.min(event.received_at_ms);
        millis_ceiling = millis_ceiling.max(event.received_at_ms);
        microsecond_floor = min_option(microsecond_floor, event.received_at_us);
        microsecond_ceiling = max_option(microsecond_ceiling, event.received_at_us);
        write_event(&mut payload, event, &dictionaries, side_code)?;
    }
    let checksum_value = crc32fast::hash(&payload);
    let compressed = zstd::encode_all(payload.as_slice(), zstd_level)
        .context("compressing CLOB replay block")?;
    Ok(EncodedClobReplayBlock {
        min_received_at_ms: millis_floor,
        max_received_at_ms: millis_ceiling,
        min_received_at_us: microsecond_floor,
        max_received_at_us: microsecond_ceiling,
        row_count: u32_len(events.len(), "row count")?,
        up_rows,
        down_rows,
        schema_version: CLOB_REPLAY_BLOCK_SCHEMA_VERSION,
        compressed_bytes: compressed.len() as u64,
        uncompressed_bytes: payload.len() as u64,
        checksum: format!("{checksum_value:08x}"),
        payload: compressed,
    })
}

/// Decode one compressed `SQLite` block payload.
pub fn decode_payload(
    compressed_payload: &[u8],
    expected_checksum: &str,
) -> anyhow::Result<Vec<DecodedClobReplayEvent>> {
    let payload =
        zstd::decode_all(compressed_payload).context("decompressing CLOB replay block")?;
    let actual_checksum = format!("{:08x}", crc32fast::hash(&payload));
    if actual_checksum != expected_checksum {
        bail!(
            "CLOB replay block checksum mismatch: expected {expected_checksum}, got {actual_checksum}"
        );
    }
    let mut reader = PayloadReader::new(&payload);
    reader.expect_magic(MAGIC)?;
    let schema_version = reader.read_u32()?;
    if schema_version != CLOB_REPLAY_BLOCK_SCHEMA_VERSION {
        bail!("unsupported CLOB replay block schema version: {schema_version}");
    }
    let dictionaries = Dictionaries::read_from(&mut reader)?;
    let row_count = reader.read_u32()?;
    let mut rows = Vec::with_capacity(row_count as usize);
    for _ in 0..row_count {
        rows.push(read_event(&mut reader, &dictionaries)?);
    }
    reader.finish()?;
    Ok(rows)
}

/// Return whether one event can be represented in a CLOB replay block.
pub fn is_blockable_clob_event(event: &FeedEvent) -> bool {
    matches!(event.source.as_str(), "clob_up" | "clob_down")
        && matches!(
            event.event_type.as_str(),
            "book" | "price_change" | "best_bid_ask"
        )
        && event.best_bid.is_some()
        && event.best_ask.is_some()
        && event.bid_size.is_some()
        && event.ask_size.is_some()
}

#[derive(Debug, Clone, Default)]
struct Dictionaries {
    source_topics: Vec<String>,
    connection_ids: Vec<String>,
    sequence_keys: Vec<String>,
    market_ids: Vec<String>,
    asset_ids: Vec<String>,
}

impl Dictionaries {
    /// Build all dictionaries needed for one block.
    fn from_events(events: &[FeedEvent]) -> Self {
        Self {
            source_topics: collect_dictionary(
                events
                    .iter()
                    .filter_map(|event| event.source_topic.as_deref()),
            ),
            connection_ids: collect_dictionary(
                events
                    .iter()
                    .filter_map(|event| event.connection_id.as_deref()),
            ),
            sequence_keys: collect_dictionary(
                events
                    .iter()
                    .filter_map(|event| event.sequence_key.as_deref()),
            ),
            market_ids: collect_dictionary(
                events.iter().filter_map(|event| event.market_id.as_deref()),
            ),
            asset_ids: collect_dictionary(
                events.iter().filter_map(|event| event.asset_id.as_deref()),
            ),
        }
    }

    /// Read dictionaries from one binary payload.
    fn read_from(reader: &mut PayloadReader<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            source_topics: reader.read_dictionary()?,
            connection_ids: reader.read_dictionary()?,
            sequence_keys: reader.read_dictionary()?,
            market_ids: reader.read_dictionary()?,
            asset_ids: reader.read_dictionary()?,
        })
    }
}

/// Collect sorted unique string values into a dictionary.
fn collect_dictionary<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Write all dictionaries to one payload.
fn write_dictionaries(payload: &mut Vec<u8>, dictionaries: &Dictionaries) -> anyhow::Result<()> {
    write_dictionary(payload, &dictionaries.source_topics)?;
    write_dictionary(payload, &dictionaries.connection_ids)?;
    write_dictionary(payload, &dictionaries.sequence_keys)?;
    write_dictionary(payload, &dictionaries.market_ids)?;
    write_dictionary(payload, &dictionaries.asset_ids)?;
    Ok(())
}

/// Write one dictionary to a payload.
fn write_dictionary(payload: &mut Vec<u8>, values: &[String]) -> anyhow::Result<()> {
    write_u32(payload, u32_len(values.len(), "dictionary length")?);
    for value in values {
        write_u32(payload, u32_len(value.len(), "string length")?);
        payload.extend_from_slice(value.as_bytes());
    }
    Ok(())
}

/// Write one CLOB event to a payload.
fn write_event(
    payload: &mut Vec<u8>,
    event: &FeedEvent,
    dictionaries: &Dictionaries,
    side_code: u8,
) -> anyhow::Result<()> {
    let best_bid = required_f64(event.best_bid, "best_bid")?;
    let best_ask = required_f64(event.best_ask, "best_ask")?;
    let bid_size = required_f64(event.bid_size, "bid_size")?;
    let ask_size = required_f64(event.ask_size, "ask_size")?;
    write_u64(payload, event.received_at_ms);
    write_u64(payload, event.event_at_ms);
    write_optional_u64(payload, event.received_at_us);
    write_optional_u64(payload, event.event_at_us);
    write_u8(payload, side_code);
    write_u8(payload, event_type_code(&event.event_type)?);
    write_dictionary_code(
        payload,
        &dictionaries.source_topics,
        event.source_topic.as_deref(),
    )?;
    write_dictionary_code(
        payload,
        &dictionaries.connection_ids,
        event.connection_id.as_deref(),
    )?;
    write_dictionary_code(
        payload,
        &dictionaries.sequence_keys,
        event.sequence_key.as_deref(),
    )?;
    write_dictionary_code(
        payload,
        &dictionaries.market_ids,
        event.market_id.as_deref(),
    )?;
    write_dictionary_code(payload, &dictionaries.asset_ids, event.asset_id.as_deref())?;
    write_f64(payload, best_bid);
    write_f64(payload, best_ask);
    write_f64(payload, bid_size);
    write_f64(payload, ask_size);
    write_optional_f64(payload, event.microprice);
    write_u8(payload, fidelity_code(event.fidelity));
    Ok(())
}

/// Read one CLOB event from a payload.
fn read_event(
    reader: &mut PayloadReader<'_>,
    dictionaries: &Dictionaries,
) -> anyhow::Result<DecodedClobReplayEvent> {
    let decoded_receive_ms = reader.read_u64()?;
    let decoded_event_ms = reader.read_u64()?;
    let decoded_receive_micro = reader.read_optional_u64()?;
    let decoded_event_micro = reader.read_optional_u64()?;
    let source = source_from_side_code(reader.read_u8()?)?.to_string();
    let event_type = event_type_from_code(reader.read_u8()?)?.to_string();
    let source_topic = reader.read_dictionary_value(&dictionaries.source_topics)?;
    let connection_id = reader.read_dictionary_value(&dictionaries.connection_ids)?;
    let sequence_key = reader.read_dictionary_value(&dictionaries.sequence_keys)?;
    let market_id = reader.read_dictionary_value(&dictionaries.market_ids)?;
    let asset_id = reader.read_dictionary_value(&dictionaries.asset_ids)?;
    let best_bid = reader.read_f64()?;
    let best_ask = reader.read_f64()?;
    let bid_size = reader.read_f64()?;
    let ask_size = reader.read_f64()?;
    let microprice = reader.read_optional_f64()?;
    let fidelity = fidelity_from_code(reader.read_u8()?)?;
    Ok(DecodedClobReplayEvent {
        received_at_ms: decoded_receive_ms,
        event_at_ms: decoded_event_ms,
        received_at_us: decoded_receive_micro,
        event_at_us: decoded_event_micro,
        source,
        event_type,
        source_topic,
        connection_id,
        sequence_key,
        market_id,
        asset_id,
        best_bid,
        best_ask,
        bid_size,
        ask_size,
        microprice,
        fidelity,
    })
}

/// Return a required floating point value.
fn required_f64(value: Option<f64>, field: &str) -> anyhow::Result<f64> {
    value.with_context(|| format!("blockable CLOB event missing {field}"))
}

/// Return the compact side code for one source.
fn side_code(source: &str) -> anyhow::Result<u8> {
    match source {
        "clob_up" => Ok(1),
        "clob_down" => Ok(2),
        other => bail!("unsupported CLOB source for block storage: {other}"),
    }
}

/// Return the source label for one side code.
fn source_from_side_code(code: u8) -> anyhow::Result<&'static str> {
    match code {
        1 => Ok("clob_up"),
        2 => Ok("clob_down"),
        other => bail!("unsupported CLOB side code: {other}"),
    }
}

/// Return the compact event type code.
fn event_type_code(event_type: &str) -> anyhow::Result<u8> {
    match event_type {
        "book" => Ok(1),
        "price_change" => Ok(2),
        "best_bid_ask" => Ok(3),
        other => bail!("unsupported CLOB event type for block storage: {other}"),
    }
}

/// Return the event type for one compact code.
fn event_type_from_code(code: u8) -> anyhow::Result<&'static str> {
    match code {
        1 => Ok("book"),
        2 => Ok("price_change"),
        3 => Ok("best_bid_ask"),
        other => bail!("unsupported CLOB event type code: {other}"),
    }
}

/// Return the compact fidelity code.
fn fidelity_code(fidelity: ReplayFidelity) -> u8 {
    match fidelity {
        ReplayFidelity::RawEvent => 1,
        ReplayFidelity::LegacySnapshot => 2,
    }
}

/// Return the replay fidelity for one compact code.
fn fidelity_from_code(code: u8) -> anyhow::Result<ReplayFidelity> {
    match code {
        1 => Ok(ReplayFidelity::RawEvent),
        2 => Ok(ReplayFidelity::LegacySnapshot),
        other => bail!("unsupported replay fidelity code: {other}"),
    }
}

/// Write one optional dictionary code.
fn write_dictionary_code(
    payload: &mut Vec<u8>,
    dictionary: &[String],
    value: Option<&str>,
) -> anyhow::Result<()> {
    let Some(value) = value else {
        write_u32(payload, NONE_U32);
        return Ok(());
    };
    let lookup = dictionary_lookup(dictionary);
    let Some(code) = lookup.get(value).copied() else {
        bail!("dictionary value was not collected: {value}");
    };
    write_u32(payload, code);
    Ok(())
}

/// Build one dictionary lookup.
fn dictionary_lookup(dictionary: &[String]) -> BTreeMap<&str, u32> {
    dictionary
        .iter()
        .enumerate()
        .filter_map(|(index, value)| u32::try_from(index).ok().map(|code| (value.as_str(), code)))
        .collect()
}

/// Return the minimum over optional values.
fn min_option(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

/// Return the maximum over optional values.
fn max_option(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.max(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

/// Convert a length to `u32`.
fn u32_len(len: usize, label: &str) -> anyhow::Result<u32> {
    u32::try_from(len).with_context(|| format!("{label} does not fit in u32"))
}

/// Write one u8.
fn write_u8(payload: &mut Vec<u8>, value: u8) {
    payload.push(value);
}

/// Write one u32.
fn write_u32(payload: &mut Vec<u8>, value: u32) {
    payload.extend_from_slice(&value.to_le_bytes());
}

/// Write one u64.
fn write_u64(payload: &mut Vec<u8>, value: u64) {
    payload.extend_from_slice(&value.to_le_bytes());
}

/// Write one optional u64.
fn write_optional_u64(payload: &mut Vec<u8>, value: Option<u64>) {
    write_u64(payload, value.unwrap_or(NONE_U64));
}

/// Write one f64.
fn write_f64(payload: &mut Vec<u8>, value: f64) {
    payload.extend_from_slice(&value.to_le_bytes());
}

/// Write one optional f64.
fn write_optional_f64(payload: &mut Vec<u8>, value: Option<f64>) {
    match value {
        Some(value) => {
            write_u8(payload, 1);
            write_f64(payload, value);
        }
        None => write_u8(payload, 0),
    }
}

struct PayloadReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadReader<'a> {
    /// Create one payload reader.
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// Verify the expected magic prefix.
    fn expect_magic(&mut self, expected: &[u8]) -> anyhow::Result<()> {
        let bytes = self.read_bytes(expected.len())?;
        if bytes != expected {
            bail!("invalid CLOB replay block magic");
        }
        Ok(())
    }

    /// Read one dictionary.
    fn read_dictionary(&mut self) -> anyhow::Result<Vec<String>> {
        let count = self.read_u32()?;
        let mut values = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let length = self.read_u32()?;
            let bytes = self.read_bytes(length as usize)?;
            let value = std::str::from_utf8(bytes)
                .context("reading UTF-8 dictionary value")?
                .to_string();
            values.push(value);
        }
        Ok(values)
    }

    /// Read one optional dictionary value.
    fn read_dictionary_value(&mut self, dictionary: &[String]) -> anyhow::Result<Option<String>> {
        let code = self.read_u32()?;
        if code == NONE_U32 {
            return Ok(None);
        }
        let Some(value) = dictionary.get(code as usize) else {
            bail!("dictionary code out of range: {code}");
        };
        Ok(Some(value.clone()))
    }

    /// Read one u8.
    fn read_u8(&mut self) -> anyhow::Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    /// Read one u32.
    fn read_u32(&mut self) -> anyhow::Result<u32> {
        let bytes = self.read_array::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read one u64.
    fn read_u64(&mut self) -> anyhow::Result<u64> {
        let bytes = self.read_array::<8>()?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read one optional u64.
    fn read_optional_u64(&mut self) -> anyhow::Result<Option<u64>> {
        let value = self.read_u64()?;
        Ok((value != NONE_U64).then_some(value))
    }

    /// Read one f64.
    fn read_f64(&mut self) -> anyhow::Result<f64> {
        let bytes = self.read_array::<8>()?;
        Ok(f64::from_le_bytes(bytes))
    }

    /// Read one optional f64.
    fn read_optional_f64(&mut self) -> anyhow::Result<Option<f64>> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_f64()?)),
            other => bail!("invalid optional f64 presence flag: {other}"),
        }
    }

    /// Finish reading and verify no trailing bytes remain.
    fn finish(&self) -> anyhow::Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            bail!("CLOB replay block has trailing bytes")
        }
    }

    /// Read a fixed-size byte array.
    fn read_array<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        let bytes = self.read_bytes(N)?;
        let mut array = [0_u8; N];
        array.copy_from_slice(bytes);
        Ok(array)
    }

    /// Read a byte slice.
    fn read_bytes(&mut self, count: usize) -> anyhow::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .context("CLOB replay block offset overflow")?;
        if end > self.bytes.len() {
            bail!("truncated CLOB replay block payload");
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_payload, encode_events};
    use crate::types::{FeedEvent, ReplayFidelity};

    /// Build one CLOB event for block codec tests.
    fn event(index: u64, source: &str, event_type: &str, bid_size: f64) -> FeedEvent {
        FeedEvent {
            id: None,
            received_at_ms: 1_000 + index,
            event_at_ms: 900 + index,
            received_at_us: Some((1_000 + index) * 1_000 + 7),
            event_at_us: Some((900 + index) * 1_000 + 3),
            source: source.to_string(),
            event_type: event_type.to_string(),
            source_topic: Some("btc-up".to_string()),
            source_symbol: None,
            connection_id: Some("conn-1".to_string()),
            sequence_key: Some(format!("seq-{index}")),
            market_id: Some("market-1".to_string()),
            asset_id: Some(format!("asset-{source}")),
            price: None,
            trade_size: None,
            signed_quantity: None,
            best_bid: Some(0.45),
            best_ask: Some(0.55),
            bid_size: Some(bid_size),
            ask_size: Some(20.0),
            depth_bid_notional: None,
            depth_ask_notional: None,
            depth_imbalance: None,
            microprice: Some(0.51),
            payload_json: None,
            details_json: None,
            fidelity: ReplayFidelity::RawEvent,
        }
    }

    /// Verifies the block codec round trips CLOB rows exactly.
    #[test]
    fn codec_round_trips_events() {
        let events = vec![
            event(1, "clob_up", "price_change", 10.0),
            event(2, "clob_down", "best_bid_ask", 15.0),
        ];
        let block = encode_events(&events, 1).unwrap();
        let decoded = decode_payload(&block.payload, &block.checksum).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].source, "clob_up");
        assert_eq!(decoded[1].source, "clob_down");
        assert_eq!(decoded[0].bid_size, 10.0);
        assert_eq!(decoded[1].event_type, "best_bid_ask");
        assert_eq!(block.up_rows, 1);
        assert_eq!(block.down_rows, 1);
    }
}
