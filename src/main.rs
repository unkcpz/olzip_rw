use std::{
    io::{Cursor, Read, Seek, Write},
    path::PathBuf,
};

// TODO (jyu): blocking not supported with wasm target, use async version
// maybe it can because it is "not (WebAssembly and (target_os=unknown or bare-metal))"
// If the target_os is specified. But anyway much easier if it is non-blocking.
use indicatif::ProgressBar;
use ol_zip_reader::SeekableHttpReader;
use reqwest::blocking::Client;
use zip::ZipArchive;

struct PattLogger {
    path: PathBuf,
}

impl log::Log for PattLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // NOTE(jyu): crappy log implementation ;) not very performant good because I keep on open and close the
            // file handler.
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.path.clone())
                .unwrap();
            let s: String = format!("{} - {}\n", record.level(), record.args());
            file.write_all(s.as_bytes()).unwrap();
            file.flush().unwrap();
        }
    }

    fn flush(&self) {}
}

#[derive(Debug, Default, Eq, PartialEq)]
struct DebugCursor<T> {
    inner: Cursor<T>,
}

impl<T> Read for DebugCursor<T>
where
    T: AsRef<[u8]>,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        log::debug!("Read {} bytes", buf.len());
        self.inner.read(buf)
    }
}

// adding to top the debug log to file "seek_pat.txt".
impl<T> Seek for DebugCursor<T>
where
    T: AsRef<[u8]>,
{
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        // log seek pattern
        let curr_pos = self.inner.position();
        match pos {
            std::io::SeekFrom::Start(pos) => {
                log::debug!("SeekFromStart: At {}; seek: 0 -> {}", curr_pos, pos);
            }
            std::io::SeekFrom::End(pos) => {
                log::debug!(
                    "SeekFromEnd: At {}; seek: {} <- end",
                    curr_pos,
                    curr_pos as i64 + pos
                );
            }
            std::io::SeekFrom::Current(offset) => {
                log::debug!(
                    "SeekFromCurrent: At {}; seek: -> {} <- end",
                    curr_pos,
                    curr_pos as i64 + offset
                );
            }
        }
        self.inner.seek(pos)
    }
}

fn fetch_from_uri(client: Client, uri: &str, len: usize) -> Vec<u8> {
    // TODO (jyu): check if it is a valid zip. abort if it is not.
    let mut case: Vec<u8> = Vec::with_capacity(len);

    let pb = ProgressBar::new(len as u64);
    let mut response = client.get(uri).send().unwrap();
    let total = response.content_length().unwrap_or(0);
    debug_assert!(total == len as u64);

    // 64KB buffer
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        case.write_all(&buf[..n]).unwrap();

        pb.inc(n as u64);
    }
    pb.finish();

    case
}

fn list_zip<T>(rd: T) -> anyhow::Result<()>
where
    T: Seek + Read,
{
    // TODO (jyu): for performance, there is zipArchive::file_names which return an iter of &str
    log::debug!("---------------------------");
    log::debug!("create archive");
    log::debug!("---------------------------");
    let mut zip = ZipArchive::new(rd).unwrap();

    log::debug!("List zip start\n");

    // TODO (jyu): folder hierachy with proper indentation and unix_mode, size etc.
    println!("| idx | filename |u/c size (bytes) | unix_mode |");
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        println!(
            "{} | {} | {} / {} ||  ",
            i,
            file.name(),
            file.compressed_size(),
            file.size(),
        );
    }
    log::debug!("List zip finish\n");
    println!();

    Ok(())
}

fn get_file_content_from_zip_by_index<T>(buf: T, idx: usize) -> anyhow::Result<Vec<u8>>
where
    T: Seek + Read,
{
    let mut zip = ZipArchive::new(buf).unwrap();
    log::debug!("---------------------------");
    log::debug!("extract file start");
    log::debug!("---------------------------");
    let mut file = zip.by_index(idx)?;

    {
        log::debug!("---------------------------");
        log::debug!("file size check");
        log::debug!("---------------------------");
        let _ = file.size();
        log::debug!("---------------------------");
        log::debug!("file size end");
        log::debug!("---------------------------");
    }

    let mut out = vec![];
    std::io::copy(&mut file, &mut out)?;
    log::debug!("---------------------------");
    log::debug!("extract file finish");
    log::debug!("---------------------------");

    // TODO (jyu): oh, it is a zip file again? loop over the processes
    // TODO (jyu): need password, ask to input.

    Ok(out)
}

fn main() -> anyhow::Result<()> {
    let logger_path = std::env::current_dir().unwrap().join("seek_pat.txt");
    let logger = PattLogger { path: logger_path };
    log::set_boxed_logger(Box::new(logger)).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    let args: Vec<String> = std::env::args().collect();
    if args.is_empty() {
        panic!("require uri to zip as the first CLI arg");
    }

    let uri = args[1].clone();

    // collect of URIs for test
    // zip on codeberg, HTTP range supported (~6 Mb)
    // - "https://codeberg.org/jyu/ol_zip_reader_fixture/archive/main.zip";

    // zenodo example, large (120MB) but no no HTTP range request support, fail to get the content,
    // why??
    // - "https://zenodo.org/api/records/22027549/files/open-and-sustainable/prismaid-v0.17.1.zip/content";

    // full content fetch: for cases zip < 10k
    // - "https://inria.hal.science/hal-02355563v1/document";

    // need at least two HTTP request calls:
    // 0. get the header
    // 1. content (if fetch whole bytes of remote zip)
    // +x. range requests
    // {
    //     let mut zip_reader = SeekableHttpReader::new(&uri);
    // }

    // {
    // if fetch all bytes:
    let raw_request = false;
    if raw_request {
        let client = Client::new();
        let response = client.head(&uri).send().unwrap();
        let len: usize = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|hv| hv.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap();
        let zip_reader = fetch_from_uri(client, &uri, len);
        // write to a file for experiment purpose
        std::fs::write("tmp.zip", &zip_reader).unwrap();
    }

    //
    let content = std::fs::read("tmp.zip").unwrap();
    log::debug!("total len: {}\n", content.len());

    let zip_reader = Cursor::new(content);
    let mut zip_reader = DebugCursor { inner: zip_reader };

    // }

    // index

    loop {
        println!("help:\n  'ls' to list folder, 'quit' to quit\n");
        let mut inp = String::new();
        std::io::stdin().read_line(&mut inp)?;

        match inp.trim_end() {
            "quit" => {
                break;
            }
            "ls" => {
                let _ = list_zip(&mut zip_reader);
                println!("help:\n  input the index on which file you wnat to preview:\n");
            }
            ctt => {
                let idx = {
                    let idx = ctt.parse::<usize>().unwrap();
                    inp.clear();
                    idx
                };

                let content = get_file_content_from_zip_by_index(&mut zip_reader, idx).unwrap();
                let s = String::from_utf8(content).unwrap();
                println!("{}\n\n", s);
            }
        }
    }
    println!("quit read zip exit 0;;");

    Ok(())
}
