use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use std::io::Read;

use crate::{SeekableHttpReader, ZipEntryMeta};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();
}

fn set(obj: &Object, key: &str, value: JsValue) -> Result<(), JsValue> {
    Reflect::set(obj, &JsValue::from_str(key), &value)?;
    Ok(())
}

fn entry_to_js(e: &ZipEntryMeta) -> Result<JsValue, JsValue> {
    let o = Object::new();
    set(&o, "name", JsValue::from_str(&e.name))?;
    set(&o, "compressedSize", JsValue::from(e.compressed_size))?;
    set(&o, "uncompressedSize", JsValue::from(e.uncompressed_size))?;
    set(&o, "crc32", JsValue::from(e.crc32))?;
    set(&o, "localHeaderOffset", JsValue::from(e.local_header_offset))?;
    Ok(o.into())
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Holds the reader (and its cache) between calls. Worker only.
#[wasm_bindgen]
pub struct ZipSource {
    reader: SeekableHttpReader,
    metas: Vec<ZipEntryMeta>,
}

#[wasm_bindgen]
impl ZipSource {
    /// HEAD + tail + central directory happen here, once.
    #[wasm_bindgen(constructor)]
    pub fn new(url: &str) -> Result<ZipSource, JsValue> {
        let reader = SeekableHttpReader::try_new(url).map_err(err)?;
        let metas = reader.entries().map_err(err)?;
        Ok(ZipSource { reader, metas })
    }

    /// Returns { names: string[], entries: {...}[] }
    pub fn list(&mut self) -> Result<JsValue, JsValue> {
        // borrows the reader, so the cache is kept; ZipArchive::new only hits cached bytes
        let archive = zip::ZipArchive::new(&mut self.reader).map_err(err)?;

        let names = Array::new();
        for n in archive.file_names() {
            names.push(&JsValue::from_str(n));
        }
        let entries = Array::new();
        for m in &self.metas {
            entries.push(&entry_to_js(m)?);
        }

        let result = Object::new();
        set(&result, "names", names.into())?;
        set(&result, "entries", entries.into())?;
        Ok(result.into())
    }

    /// Decompressed file contents (CRC verified by the zip crate).
    pub fn fetch_entry(&mut self, name: &str) -> Result<Vec<u8>, JsValue> {
        let meta = self
            .metas
            .iter()
            .find(|m| m.name == name)
            .cloned()
            .ok_or_else(|| JsValue::from_str(&format!("entry not found: {name}")))?;

        self.reader.prefetch_entry(&meta).map_err(err)?;

        let mut archive = zip::ZipArchive::new(&mut self.reader).map_err(err)?;
        let mut file = archive.by_name(name).map_err(err)?;
        let mut out = Vec::with_capacity(meta.uncompressed_size as usize);
        file.read_to_end(&mut out).map_err(err)?;
        Ok(out)
    }
}