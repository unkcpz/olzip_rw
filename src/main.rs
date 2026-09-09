use std::io::{Cursor, Read, Seek, Write};

// TODO (jyu): blocking not supported with wasm target, use async version
// maybe it can because it is "not (WebAssembly and (target_os=unknown or bare-metal))"
// If the target_os is specified. But anyway much easier if it is non-blocking.
use indicatif::ProgressBar;
use ol_zip_reader::SeekableHttpReader;
use reqwest::blocking::Client;
use zip::ZipArchive;

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
    let mut zip = ZipArchive::new(rd).unwrap();

    // TODO (jyu): folder hierachy with proper indentation and unix_mode, size etc.
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        println!("idx: {}, Filename: {}", i, file.name());
    }
    println!();

    Ok(())
}

fn get_file_content_from_zip_by_index<T>(buf: T, idx: usize) -> anyhow::Result<Vec<u8>>
where
    T: Seek + Read,
{
    let mut zip = ZipArchive::new(buf).unwrap();
    let mut file = zip.by_index(idx)?;
    let mut out = vec![];
    std::io::copy(&mut file, &mut out)?;

    // TODO (jyu): oh, it is a zip file again? loop over the processes
    // TODO (jyu): need password, ask to input.

    Ok(out)
}

fn main() -> anyhow::Result<()> {
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
    let mut zip_reader = SeekableHttpReader::new(&uri);

    {
        // if fetch all bytes:
        // let client = Client::new();
        // let response = client.head(&uri).send().unwrap();
        // let len: usize = response
        //     .headers()
        //     .get(reqwest::header::CONTENT_LENGTH)
        //     .and_then(|hv| hv.to_str().ok())
        //     .and_then(|s| s.parse::<usize>().ok())
        //     .unwrap();
        // let zip_reader = fetch_from_uri(client, &uri, len);
        // let mut zip_reader = Cursor::new(zip_reader);
    }

    let _ = list_zip(&mut zip_reader);

    // index
    println!("input the index on which file you wnat to previet:");

    let mut inp = String::new();
    std::io::stdin().read_line(&mut inp)?;
    let inp = inp.trim_end().parse::<usize>().unwrap();

    let content = get_file_content_from_zip_by_index(&mut zip_reader, inp).unwrap();
    let s = String::from_utf8(content).unwrap();
    println!("{}", s);

    Ok(())
}
