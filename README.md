# Online zip reader (`ol_zip_reader`)

Read a zip from online URI.
The tool is targeting to inspect an online zip URI and read a single file out without loading the full zip into memory.

- If range HTTP support then the memory footprints can be kept low with range fetching.
- Otherwise, only zip file size < x Mb will be download and load to memory to be read. (x is setable from caller and recommended to be less than 1 Mb)

## License

All contributions must retain this attribution.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

