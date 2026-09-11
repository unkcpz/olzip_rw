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
- the inner file can return as seekable therefore impl back to the zip for further unzip.
- limit the scope to only read, will not modify the zip.

## Cache design

The key design of the library is how to efficiently fetch range bytes and how to store them in the cache for revisit.

- seek will move the file pointer but do not need the bytes immediatly 
- when list the folder, the read happens at the tail.
- the lib should able to locate the cental directory and store those bytes in a long-term cache which has the lifetime cross whole usage session.
- other bytes cached in a btreemap(??) so the more use the more cached and less used will be pushed out from the cache memory. 
- two type of places to store the fetched bytes require pre-allocate of two block of memory (can I do it staticlly, which can be nice for wasm maybe?)
- the caching pattern then is:
    - the arena allocation for all bytes caching, allocate 11 Mb in total (these days 11 Mb ram usage in browser seems nothing?)
    - magic find the EOCD, store it in the head of arena memory, 
    - I'd assume 1024K for this section, if ever the EODC exceed 1024K (very unlikely), abort and tell user it is a super large folder and ask to recheck the source.
    - The future bytes will goes to the second area. from 1024 kB.  
    - need an extra buffer 4k to buffer read the bytes from the caching and combine with the newly read (or all use cache as a middle man). 
    - !! caveat: if the file to read is already > cache space, then every such read will override the cache and cause the future cache miss.
    - So maybe if this become an issue, the memory alloctation model need to be decided after the first scan of folder structure. Say "there are a lot of > 10M files, allocate a larger arena for caching"
    - The caching is to solve the problem of too many HTTP range requests.
- If file is too large (larger than e.g. 30% (??) of total buffer size), I'll get it directly through a HTTP range call. Because otherwise it will override the buffer and cause more cache miss.
    - it is usually fine because assume we have 10Mb for the caching, 30% is 3MB file, and for an 3Mb file it makes sense to using one HTTP call to get the file.

## Usage

The tool is supposed to be used as a JS library.

... npm install, JS call.

## roadmap

### reader part
- [x] Basic ZipArchieve read from a buff.
- [x] uri as the first cli input.
- [x] can read/inspect a single file by index.
- [x] example fixture repo for quick test
- [ ] TUI?
- [ ] A cli inquery app, stdin, stdout interactive mode.
- [ ] list files with hirerachy.
- [x] list files with more detail information.
- [ ] recursive zip inspect if the inner file is a zip.
- [ ] !! http range read for large zip file.
    - [x] investigate the zip visiting pattern 
    - [ ] a implementation without using cache
    - [ ] a dummy cache, FIFO.
    - [ ] arena for cache, btreemap.
- [x] progress bar when making HTTP request to download the zip.
    - [ ] use trickle (https://github.com/mariusae/trickle) to manually test the behavior in slow network
    - [x] seek bytes visit pattern statistic, prepare for caching machenism.
- [ ] bench on the ram footprint to see the range call works and the force full buff use a lot of ram.
- [ ] second arg to force the full download or force the HTTP range call.
- [ ] wasm binding and JS interface for APIs.
- [ ] full wasm app or JS app calling wasm interfaces?

### writer part

- [ ] download with streaming.
- [ ] how to expose API to grpc?
- [ ] can I avoid `async_zip`?

## For developers

- wasm build, 
- benchmarking
- JS glue
- release note

## License

All contributions must retain this attribution.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

