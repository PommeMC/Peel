//! The viewer's bridge to `peel-decode`: the browser hands over a trace's
//! bytes and gets back the same timeline `peel dump --json` prints.

use peel_decode::Loaded;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Trace {
    loaded: Loaded,
    timeline: String,
}

#[wasm_bindgen]
impl Trace {
    #[wasm_bindgen(constructor)]
    pub fn open(bytes: &[u8]) -> Result<Trace, JsError> {
        let loaded = peel_decode::load(bytes).map_err(|err| JsError::new(&err.to_string()))?;
        let timeline = serde_json::to_string(&loaded.timeline())?;
        Ok(Trace { loaded, timeline })
    }

    /// The timeline as JSON, in the shape of `peel dump --json`.
    pub fn timeline(&self) -> String {
        self.timeline.clone()
    }

    /// The uncompressed frame of the record at `index`.
    pub fn frame(&self, index: usize) -> Option<Vec<u8>> {
        self.loaded.records.get(index).map(|r| r.frame.clone())
    }

    pub fn warning(&self) -> Option<String> {
        self.loaded.warning()
    }
}
