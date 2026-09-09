/*
* - Note: cache design
*
* Should the cache store instead of the pure bytes stream of response, I use a hashmap to store
* multiple range HTTP requests responses?
* If the full file is fetched, it is fetched into the cache storage and store as the first (and
* only) cache instance.
*/

use std::io::{Read, Seek, SeekFrom};

use thiserror::Error;

// By default fetch next 20 files into cache in range call accept.
struct Fetcher {
    client: reqwest::blocking::Client,
    uri: String,
    content_length: u64,
    accept_ranges: bool,
}

/// Errors that may be returned by a [`RangeFetcher`].
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("Initial HTTP HEAD command failed")]
    HttpHead(#[source] reqwest::Error),
    #[error("HTTP server did not specify a Content-Length header")]
    NoContentLength,
    #[error("HTTP resource was zero length")]
    EmptyContentLength,
}

impl Fetcher {
    fn new(uri: &str) -> Result<Self, Error> {
        let client = reqwest::blocking::Client::new();

        // make the first header call to guess the default config
        let response = client.head(uri).send().map_err(Error::HttpHead)?;
        let content_length: u64 = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|hv| hv.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or(Error::NoContentLength)?;
        if content_length == 0 {
            return Err(Error::EmptyContentLength);
        }
        let accept_ranges = response
            .headers()
            .contains_key(reqwest::header::ACCEPT_RANGES);

        Ok(Fetcher {
            client,
            uri: uri.to_string(),
            content_length,
            accept_ranges,
        })
    }

    fn fetch_range(self, start: u64, end: u64) -> reqwest::blocking::Response {
        todo!()
    }
}

pub struct SeekableHttpReader {
    uri: String,
    pos: u64,
}

impl SeekableHttpReader {
    pub fn new(uri: &str) -> Self {
        SeekableHttpReader {
            uri: uri.to_string(),
            pos: 0,
        }
    }

    fn len(&self) -> u64 {
        todo!()
    }
}

impl Seek for SeekableHttpReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(pos) => {
                // rewind seek from max_offset a.k.a length
                let positive_pos: u64 = (-pos).try_into().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Unsupported, "Seeked beyond end")
                })?;
                self.len()
                    .checked_sub(positive_pos)
                    .ok_or(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Rewind too far",
                    ))?
            }
            SeekFrom::Current(offset_from_pos) => {
                // both forward and backward seek, take care by using the overflow error
                let offset_from_pos_u64: Result<u64, _> = offset_from_pos.try_into();
                match offset_from_pos_u64 {
                    Ok(positive_offset) => self.pos + positive_offset,
                    Err(_) => {
                        let negative_offset = -offset_from_pos as u64;
                        self.len()
                            .checked_sub(negative_offset)
                            .ok_or(std::io::Error::new(
                                std::io::ErrorKind::Unsupported,
                                "Rewind too far",
                            ))?
                    }
                }
            }
        };

        Ok(self.pos)
    }
}

impl Read for SeekableHttpReader {
    // NOTE: (jyu) The implementation is borrowed from ripunzip, by looking at `SeekableHttpReaderEngine::read`.
    //
    /// Read some data, ideally from the cache of pre-read blocks, but
    /// otherwise from the underlying HTTP stream.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        log::debug!("Read: requested position 0x{:x}.", self.pos);
        if self.pos == self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "read beyond end of stream",
            ));
        }

        // TODO: (jyu) cache lock and cache read

        // state.cache_misses += 1;

        todo!()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     use zip::ZipArchive;
//
//     #[test]
//     fn run_init() {
//         let uri = "https://zenodo.org/api/records/22027549/files/open-and-sustainable/prismaid-v0.17.1.zip/content";
//         let reader = SeekableHttpReader::new(uri);
//         let myzip = ZipArchive::new(reader).unwrap();
//
//         let files = myzip
//             .file_names()
//             .map(|s| s.to_string());
//             .collect::<Vec<String>>();
//         dbg!(files);
//     }
//
//     #[test]
//     fn run_fetch() {
//         let zip_uri = "https://inria.hal.science/hal-02355563v1/document";
//         let cache_cell: Vec<u8> = fetch_from_uri(zip_uri, content_length);
//
//
//     }
// }
//
// /*
// * Maybe deprecated notes:
// *
// * - Note: rayon v.s async
// * ripunzip use rayon and blocking reqwest.
// * In my implementation, because the use case is focus on HTTP range call and to maximizely optimize it.
// * I use async to be able to firing multiple HTTP range calls to get a larger caching pool to
// * improve cache hit. This can cause higher number of http calls therefore with the risk of hitting
// * rate limit easier.
// *
// */
