# Online zip reader (`ol_zip_reader`)

A Webassembly tool to read a zip from online URI, with JS package provided on npm.
The tool is targeting to inspect an online zip URI and read a single file out without loading the full zip into memory.

- If range HTTP is supported then the memory footprints can be kept low with range fetching.
    - Using range HTTP calls might lead to lots of HTTP requests, pre-fetch the whole file or pre-fetch more bytes can help.
    - If file is very small (<10 kB), even the range HTTP call is supported, do load the full zip into memory (this could be just the cache storage?).
    - For range call, a cache pool is created store the prefetched bytes.
    - A proper caching mechanism is required to optimize the cache hit.
    - The lifetime of cache storage is as longer as the full zip inspectation session.
- If range HTTP is not supported, only zip file size < x Mb will be download and load to memory to be read. (x is setable from caller and recommended to be less than 1 Mb)
    - The download will always showing detail progress info.
    - I provide the option if user insist to download and loading the whole file into memory.

## Usage

The tool is supposed to be used as a JS library.

... npm install, JS call.

## For developers

- wasm build, 
- benchmarking
- JS glue
- release note

## License

All contributions must retain this attribution.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

