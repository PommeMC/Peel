//! The viewer's bridge to `peel-decode`: the browser hands over a trace's
//! bytes and gets back the same timeline `peel dump --json` prints.

use peel_decode::Loaded;
use peel_decode::diff::Options;
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

    /// The decoded fields of the record at `index`, as JSON.
    pub fn fields(&self, index: usize) -> Result<Option<String>, JsError> {
        Ok(self
            .loaded
            .fields(index)
            .map(|decoded| serde_json::to_string(&decoded))
            .transpose()?)
    }

    /// What the trace decodes and where decoding stopped, as JSON, in the
    /// shape of `peel coverage --json`.
    pub fn coverage(&self) -> Result<String, JsError> {
        Ok(serde_json::to_string(&peel_decode::coverage::coverage(
            &self.loaded,
        ))?)
    }

    pub fn warning(&self) -> Option<String> {
        self.loaded.warning()
    }
}

/// Compares `trace` against `reference` with `options` as JSON, returning
/// the diff as JSON in the shape of `peel diff --json`.
#[wasm_bindgen]
pub fn diff(reference: &Trace, trace: &Trace, options: &str) -> Result<String, JsError> {
    let options: Options = serde_json::from_str(options)?;
    let diff = peel_decode::diff::diff(&reference.loaded, &trace.loaded, &options)?;
    Ok(serde_json::to_string(&diff)?)
}
