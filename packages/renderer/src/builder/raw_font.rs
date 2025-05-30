use js_sys::Uint8Array;
use reflexo_typst::{error::prelude::*, Bytes};
use wasm_bindgen::prelude::*;

use crate::TypstRendererBuilder;

#[wasm_bindgen]
impl TypstRendererBuilder {
    pub async fn add_raw_font(&mut self, font_buffer: Uint8Array) -> Result<()> {
        self.add_raw_font_internal(font_buffer.to_vec().into());
        Ok(())
    }
}

impl TypstRendererBuilder {
    pub fn add_raw_font_internal(&mut self, font_buffer: Bytes) {
        self.searcher.add_font_data(font_buffer);
    }
}
