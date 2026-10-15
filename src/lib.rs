use bytes::Bytes;
use range_cache::{CacheCapacity, RangeCache, RangeError};
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use thiserror::Error;
use url::Url;

mod transport;
use transport::HttpClient;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
const CACHE_BYTES: usize = 64 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const CACHE_BYTES: usize = 1024 * 1024 * 1024 / 10;

/// Errors that may be returned by a [`RangeFetcher`].
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("Invalid URI")]
    InvalidUri,
    #[error("Initial HTTP HEAD command failed: {0}")]
    HttpHead(String),
    #[error("HTTP range GET request failed: {0}")]
    HttpRangeGet(String),
    #[error("HTTP header does not contain Accept-Ranges")]
    HttpNoAcceptRanges,
    #[error("HTTP server did not specify a Content-Length header")]
    NoContentLength,
    #[error("HTTP resource was zero length")]
    EmptyContentLength,
    #[error("HTTP content-length could not be interpreted as a number")]
    InvalidContentLengthValue(#[source] std::num::ParseIntError),
    #[error("HTTP resource had a different Content-Length than requested")]
    LengthMismatch,
    #[error("HTTP server did not specify a Content-Range header")]
    NoContentRange,
    #[error("Content-Range header could not be parsed correctly")]
    InvalidContentRangeFormat,
    #[error("HTTP content-range could not be interpreted as a number")]
    InvalidContentRangeValue(#[source] std::num::ParseIntError),
    #[error("HTTP resource had a different Content-Range than requested")]
    RangeMismatch,
    #[error("Cache error on insert")]
    CacheRangeError(#[source] RangeError),
    #[error("End of central directory record not found")]
    NoEocd,
    #[error("Zip64 archives are not supported yet")]
    Zip64Unsupported,
    #[error("Invalid central directory")]
    InvalidCentralDirectory,
    #[error("Invalid local file header")]
    InvalidLocalHeader,
}

#[derive(Debug, Clone)]
pub struct ZipEntryMeta {
    pub name: String,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub crc32: u32,
    pub local_header_offset: u32,
}

struct Fetcher {
    http: HttpClient,
    uri: String,
    content_length: u64,
    accept_ranges: bool,
    cache: RangeCache<()>,
}

impl Fetcher {
    fn new(uri: &str) -> Result<Self, Error> {
        let url = Url::parse(uri).map_err(|_e| Error::InvalidUri)?;
        let host = url.host_str().ok_or(Error::InvalidUri)?.to_string();

        let http = HttpClient::new(host);

        // make the first header call to guess the default config
        let response = http.head(uri)?;

        let content_length: u64 = response
            .content_length
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or(Error::NoContentLength)?;

        log::debug!("Initial head request returned content-length {content_length}");

        if content_length == 0 {
            return Err(Error::EmptyContentLength);
        }
        let accept_ranges = response.accept_ranges;

        let cache: RangeCache<()> = RangeCache::new(CacheCapacity::Bounded(
            NonZeroUsize::new(CACHE_BYTES).expect("capacity is non-zero"),
        ));

        Ok(Fetcher {
            http,
            uri: uri.to_string(),
            content_length,
            accept_ranges,
            cache,
        })
    }

    /// Parses an EOCD record (slice must start at its signature).
    /// Returns (central directory start, end, total records).
    fn central_dir_range(&self, eocd: &[u8]) -> Result<(u64, u64, u16), Error> {
        if eocd.len() < 22 || eocd[0..4] != [0x50, 0x4b, 0x05, 0x06] {
            return Err(Error::NoEocd);
        }

        let total = u16::from_le_bytes(eocd[10..12].try_into().unwrap());
        let cd_size = u32::from_le_bytes(eocd[12..16].try_into().unwrap());
        let cd_offset = u32::from_le_bytes(eocd[16..20].try_into().unwrap());

        // zip64 archives store placeholder values here
        if total == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF {
            return Err(Error::Zip64Unsupported);
        }

        let start = cd_offset as u64;
        let end = start + cd_size as u64; // u64 math: no overflow
        if cd_size == 0 || end > self.content_length {
            return Err(Error::InvalidCentralDirectory);
        }

        log::debug!("EOCD: {total} records, CD at {start}..{end}");
        Ok((start, end, total))
    }

    fn parse_central_directory(&self, cd: &[u8], total_dir_records: u16) -> Vec<ZipEntryMeta> {
        let mut entries = Vec::with_capacity(total_dir_records as usize);
        let mut pos = 0;

        for _ in 0..total_dir_records {
            if pos + 46 > cd.len() || &cd[pos..pos + 4] != b"PK\x01\x02" {
                break;
            }

            let crc32 = u32::from_le_bytes(cd[pos + 16..pos + 20].try_into().unwrap());
            let compressed_size = u32::from_le_bytes(cd[pos + 20..pos + 24].try_into().unwrap());
            let uncompressed_size = u32::from_le_bytes(cd[pos + 24..pos + 28].try_into().unwrap());
            let file_name_length =
                u16::from_le_bytes(cd[pos + 28..pos + 30].try_into().unwrap()) as usize;
            let extra_field_length =
                u16::from_le_bytes(cd[pos + 30..pos + 32].try_into().unwrap()) as usize;
            let comment_length =
                u16::from_le_bytes(cd[pos + 32..pos + 34].try_into().unwrap()) as usize;
            let local_header_offset =
                u32::from_le_bytes(cd[pos + 42..pos + 46].try_into().unwrap());

            let name_start = pos + 46;
            let name_end = name_start + file_name_length;
            if name_end > cd.len() {
                break;
            }
            let name = String::from_utf8_lossy(&cd[name_start..name_end]).into_owned();

            entries.push(ZipEntryMeta {
                name,
                compressed_size,
                uncompressed_size,
                crc32,
                local_header_offset,
            });

            pos = name_end + extra_field_length + comment_length;
        }

        entries
    }

    /// Fetches the tail and the central directory (cached), and returns the parsed entries.
    fn read_entries(&self) -> Result<Vec<ZipEntryMeta>, Error> {
        let len = self.content_length;

        // EOCD (22 bytes) + max comment length (65535) is the farthest it can be from the end
        let tail_start = len.saturating_sub(65535 + 22);
        let tail = self.get_range(tail_start, len)?;

        // find the last EOCD signature in the tail
        let pos = tail
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
            .ok_or(Error::NoEocd)?;
        let eocd = &tail[pos..];

        let (start, end, total) = self.central_dir_range(eocd)?;

        // cache hit if the CD is inside the tail we already fetched
        let cd = self.get_range(start, end)?;
        Ok(self.parse_central_directory(&cd, total))
    }

    fn get_range(&self, start: u64, end: u64) -> Result<Bytes, Error> {
        let missing = self
            .cache
            .missing_ranges(&(), (start as usize)..(end as usize))
            .map_err(Error::CacheRangeError)?;

        let requested_bytes = if missing.is_empty() {
            let cache_hit = self
                .cache
                .get(&(), (start as usize)..(end as usize))
                .map_err(Error::CacheRangeError)?
                .ok_or(Error::RangeMismatch)?;
            //log::debug!("Read cached bytes with length {}", cache_hit.len());
            cache_hit
        } else if missing.len() == 1 {
            log::debug!("One missing range {:?}", missing);
            let gap = &missing[0];
            let fetched = self.fetch_range(gap.start as u64, gap.end as u64)?;
            if start == gap.start as u64 && end == gap.end as u64 {
                // gap and requested bytes are equal
                fetched
            } else {
                // gap and requested bytes are not equal
                // return request bytes range
                log::debug!("returning {:?}-{:?} ", start, end);
                self.get_range(start, end)?
            }
        } else {
            log::debug!("Missing ranges {:?}", missing);
            self.fetch_range(start, end)?
        };

        Ok(requested_bytes)
    }

    fn fetch_range(&self, start: u64, end: u64) -> Result<Bytes, Error> {
        /*if !self.accept_ranges {
            return Err(Error::HttpNoAcceptRanges);
        }*/

        log::debug!("About to fetch {} bytes ({}-{})", end - start, start, end - 1);

        let response = self.http.get_range(&self.uri, start, end)?;

        let content_length: u64 = response
            .content_length
            .as_deref()
            .ok_or(Error::NoContentLength)?
            .parse()
            .map_err(Error::InvalidContentLengthValue)?;

        let content_range = response
            .content_range
            .as_deref()
            .ok_or(Error::NoContentRange)?;

        let (content_range, _total) = content_range
            .strip_prefix("bytes ")
            .and_then(|s| s.split_once('/'))
            .ok_or(Error::InvalidContentRangeFormat)?;

        let (content_range_start, content_range_end) = content_range
            .split_once("-")
            .ok_or(Error::InvalidContentRangeFormat)?;

        let content_range_start = content_range_start
            .parse::<u64>()
            .map_err(Error::InvalidContentRangeValue)?;
        let content_range_end = content_range_end
            .parse::<u64>()
            .map_err(Error::InvalidContentRangeValue)?;

        if content_length != end - start {
            return Err(Error::LengthMismatch);
        }
        if start != content_range_start || content_range_end != end - 1 {
            return Err(Error::RangeMismatch);
        }

        let requested_bytes = response.body;
        if requested_bytes.len() != (end - start) as usize {
            return Err(Error::LengthMismatch);
        }

        log::debug!("Fetched {} bytes", requested_bytes.len());

        // write to cache
        let insert_outcome = self
            .cache
            .insert((), (start as usize)..(end as usize), requested_bytes.clone())
            .map_err(Error::CacheRangeError)?;
        log::debug!("Cache InsertOutcome: {:?}", insert_outcome);

        Ok(requested_bytes)
    }

    /// Returns (data_start, data_end) of the entry's compressed bytes.
    /// The local header's name/extra lengths can differ from the central directory's,
    /// so they are read from the local header itself.
    fn entry_data_range(&self, meta: &ZipEntryMeta) -> Result<(u64, u64), Error> {
        let off = meta.local_header_offset as u64;
        let hdr_end = (off + 30).min(self.content_length);
        let hdr = self.get_range(off, hdr_end)?;
        if hdr.len() < 30 || hdr[0..4] != [0x50, 0x4b, 0x03, 0x04] {
            return Err(Error::InvalidLocalHeader);
        }
        let name_len = u16::from_le_bytes(hdr[26..28].try_into().unwrap()) as u64;
        let extra_len = u16::from_le_bytes(hdr[28..30].try_into().unwrap()) as u64;

        let start = off + 30 + name_len + extra_len;
        let end = start + meta.compressed_size as u64;
        if end > self.content_length {
            return Err(Error::InvalidLocalHeader);
        }
        Ok((start, end))
    }
}

pub struct SeekableHttpReader {
    pos: u64,
    fetcher: Fetcher,
}

impl SeekableHttpReader {
    pub fn new(uri: &str) -> Self {
        SeekableHttpReader {
            pos: 0,
            fetcher: Fetcher::new(uri).unwrap(),
        }
    }

    /// Non-panicking constructor (used by the wasm entry point).
    pub(crate) fn try_new(uri: &str) -> Result<Self, Error> {
        Ok(SeekableHttpReader {
            pos: 0,
            fetcher: Fetcher::new(uri)?,
        })
    }

    /// Entry metadata parsed from the central directory (offsets, sizes, CRC).
    pub(crate) fn entries(&self) -> Result<Vec<ZipEntryMeta>, Error> {
        self.fetcher.read_entries()
    }

    /// Loads the whole entry into the cache so the zip crate's reads
    /// don't each cause their own HTTP request.
    pub(crate) fn prefetch_entry(&self, meta: &ZipEntryMeta) -> Result<(), Error> {
        let (_, end) = self.fetcher.entry_data_range(meta)?;
        let off = meta.local_header_offset as u64;
        self.fetcher.get_range(off, end)?;
        Ok(())
    }

    fn len(&self) -> u64 {
        self.fetcher.content_length
    }
}

impl Seek for SeekableHttpReader {
    fn seek(&mut self, requested_pos: std::io::SeekFrom) -> std::io::Result<u64> {
        //log::debug!("SeekFrom pos: {:?}", requested_pos);

        let new_pos = match requested_pos {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(pos) => {
                // pos is expected to be <= 0
                self.len()
                    .checked_add_signed(pos)
                    .ok_or_else(|| {
                        let msg = if pos < 0 {
                            "New seek position would be negative"
                        } else {
                            "New seek position would exceed u64 max limit"
                        };
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
                    })?
            }
            SeekFrom::Current(pos) => {
                // pos can be positive (forward), zero (stay), or negative (backward)
                self.pos
                    .checked_add_signed(pos)
                    .ok_or_else(|| {
                        let msg = if pos < 0 {
                            "New seek position would be negative"
                        } else {
                            "New seek position would exceed u64 max limit"
                        };
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
                    })?
            }
        };

        if new_pos > self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("New seek position {} would overflow {}", new_pos, self.len()),
            ));
        }

        self.pos = new_pos;

        //log::debug!("New Seek pos: {}", self.pos);
        Ok(self.pos)
    }
}

impl Read for SeekableHttpReader {
    // NOTE: (jyu) The implementation is borrowed from ripunzip, by looking at `SeekableHttpReaderEngine::read`.
    //
    /// Read some data, ideally from the cache of pre-read blocks, but
    /// otherwise from the underlying HTTP stream.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        //log::debug!("Read: requested position {}.", self.pos);
        // TODO: also check whether requested range self.pos + buf.len() would go beyond self.len()
        if self.pos == self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "read beyond end of stream",
            ));
        }

        let range = self
            .fetcher
            .get_range(self.pos, self.pos + buf.len() as u64)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let n = range.len();
        buf[..].copy_from_slice(&range[..]);
        self.pos += n as u64;
        //log::debug!("New Read pos: {}", self.pos);
        Ok(n)
    }
}

#[cfg(test)]
impl Fetcher {
    fn new_for_test(content_length: u64) -> Self {
        Fetcher {
            http: HttpClient::new("test.invalid".to_string()),
            uri: "http://test.invalid/".to_string(),
            content_length,
            accept_ranges: true,
            cache: RangeCache::new(CacheCapacity::Bounded(NonZeroUsize::new(1024).unwrap())),
        }
    }
}

#[cfg(test)]
impl SeekableHttpReader {
    fn new_for_test(content_length: u64) -> Self {
        SeekableHttpReader {
            pos: 0,
            fetcher: Fetcher::new_for_test(content_length),
        }
    }
}

// These tests use mockito and reqwest::blocking, so they run on native only.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn seek_start_within_bounds() {
        let mut r = SeekableHttpReader::new_for_test(100);
        assert_eq!(r.seek(SeekFrom::Start(50)).unwrap(), 50);
    }

    #[test]
    fn seek_start_past_end_errors() {
        let mut r = SeekableHttpReader::new_for_test(100);
        assert!(r.seek(SeekFrom::Start(500)).is_err());
    }

    #[test]
    fn seek_end_zero_lands_at_length() {
        let mut r = SeekableHttpReader::new_for_test(100);
        assert_eq!(r.seek(SeekFrom::End(0)).unwrap(), 100);
    }

    #[test]
    fn seek_end_negative_rewinds_correctly() {
        let mut r = SeekableHttpReader::new_for_test(100);
        assert_eq!(r.seek(SeekFrom::End(-22)).unwrap(), 78);
    }

    #[test]
    fn seek_end_too_far_back_errors() {
        let mut r = SeekableHttpReader::new_for_test(100);
        assert!(r.seek(SeekFrom::End(-200)).is_err());
    }

    #[test]
    fn seek_current_forward() {
        let mut r = SeekableHttpReader::new_for_test(100);
        r.seek(SeekFrom::Start(10)).unwrap();
        assert_eq!(r.seek(SeekFrom::Current(5)).unwrap(), 15);
    }

    #[test]
    fn seek_current_backward() {
        let mut r = SeekableHttpReader::new_for_test(100);
        r.seek(SeekFrom::Start(10)).unwrap();
        assert_eq!(r.seek(SeekFrom::Current(-5)).unwrap(), 5);
    }

    #[test]
    fn seek_current_backward_past_start_errors() {
        let mut r = SeekableHttpReader::new_for_test(100);
        r.seek(SeekFrom::Start(5)).unwrap();
        assert!(r.seek(SeekFrom::Current(-10)).is_err());
    }

    #[test]
    fn read_zip() {
        let mut server = mockito::Server::new();

        let _head = server
            .mock("HEAD", "/test.zip")
            .with_status(200)
            .with_header("content-length", "10")
            .with_header("accept-ranges", "bytes")
            .expect(1)
            .create();

        let _get = server
            .mock("GET", "/test.zip")
            .match_header("range", "bytes=0-4")
            .with_status(206)
            .with_header("content-length", "5")
            .with_header("content-range", "bytes 0-4/10")
            .with_body(b"hello")
            .expect(1)
            .create();

        let mut reader = SeekableHttpReader::new(&format!("{}/test.zip", server.url()));
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();

        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");

        _head.assert();
        _get.assert();
    }

    #[test]
    fn fetch_range() {
        let mut server = mockito::Server::new();

        let _head = server
            .mock("HEAD", "/test.zip")
            .with_status(200)
            .with_header("content-length", "10")
            .with_header("accept-ranges", "bytes")
            .expect(1)
            .create();

        let _get = server
            .mock("GET", "/test.zip")
            .match_header("range", "bytes=0-4")
            .with_status(206)
            .with_header("content-length", "5")
            .with_header("content-range", "bytes 0-4/10")
            .with_body(b"hello")
            .expect(1)
            .create();

        let fetcher = Fetcher::new(&format!("{}/test.zip", server.url())).unwrap();
        let range = fetcher.fetch_range(0, 5).unwrap();

        assert_eq!(range.len(), 5);
        assert_eq!(range, &(b"hello")[..]);

        _head.assert();
        _get.assert();
    }

    #[test]
    fn get_range() {
        let mut server = mockito::Server::new();

        let _head = server
            .mock("HEAD", "/test.zip")
            .with_status(200)
            .with_header("content-length", "10")
            .with_header("accept-ranges", "bytes")
            .expect(1)
            .create();

        let _get = server
            .mock("GET", "/test.zip")
            .match_header("range", "bytes=0-4")
            .with_status(206)
            .with_header("content-length", "5")
            .with_header("content-range", "bytes 0-4/10")
            .with_body(b"hello")
            .expect(1)
            .create();

        let fetcher = Fetcher::new(&format!("{}/test.zip", server.url())).unwrap();

        let missing = fetcher.cache.missing_ranges(&(), 0..5).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], 0..5);

        let range = fetcher.get_range(0, 5).unwrap();

        assert_eq!(range.len(), 5);
        assert_eq!(range, &(b"hello")[..]);

        // result already cached
        let range = fetcher.get_range(0, 5).unwrap();
        assert_eq!(range.len(), 5);
        assert_eq!(range, &(b"hello")[..]);

        let missing = fetcher.cache.missing_ranges(&(), 0..5).unwrap();
        assert_eq!(missing.len(), 0);

        _head.assert();
        _get.assert();
    }
}