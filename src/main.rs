use std::io::{Cursor, Read, Seek};

// TODO (jyu): blocking not supported with wasm target, use async version
// maybe it can because it is "not (WebAssembly and (target_os=unknown or bare-metal))"
// If the target_os is specified. But anyway much easier if it is non-blocking.
use reqwest::blocking::Client;
use zip::ZipArchive;

fn fetch_from_uri(client: Client, uri: &str, len: usize) -> Vec<u8> {
    // TODO (jyu): check if it is a valid zip. abort if it is not.
    let mut case: Vec<u8> = Vec::with_capacity(len);
    client.get(uri).send().unwrap().copy_to(&mut case).unwrap();

    case
}

fn list_zip<T>(buf: T) -> anyhow::Result<()>
where
    T: Seek + Read,
{
    // TODO (jyu): for performance, there is zipArchive::file_names which return an iter of &str
    let mut zip = ZipArchive::new(buf).unwrap();

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
    // zip on codeberg, HTTP range supported (~6 Mb)
    let zip_uri = "https://codeberg.org/jyu/ol_zip_reader_fixture/archive/main.zip";

    // zenodo example, large (120MB) but no no HTTP range request support, fail to get the content,
    // why??
    // let zip_uri = "https://zenodo.org/api/records/22027549/files/open-and-sustainable/prismaid-v0.17.1.zip/content";

    // full content fetch: for cases zip < 10k
    // let zip_uri = "https://inria.hal.science/hal-02355563v1/document";
    let client = Client::new();
    let response = client.head(zip_uri).send().unwrap();
    let len: usize = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap();

    let buff = fetch_from_uri(client, zip_uri, len);
    let mut buff = Cursor::new(buff);

    let _ = list_zip(&mut buff);

    // index
    println!("input the index on which file you wnat to previet:");

    let mut inp = String::new();
    std::io::stdin().read_line(&mut inp)?;
    let inp = inp.trim_end().parse::<usize>().unwrap();

    let content = get_file_content_from_zip_by_index(&mut buff, inp).unwrap();
    let s = String::from_utf8(content).unwrap();
    println!("{}", s);

    Ok(())
}
