#![no_main]

use crate::{
    exports::componentized::component::types::{Component, Error},
    exports::componentized::component::wasm_loader::Guest,
};
use wit_bindgen::rt::async_support::StreamReader;

pub(crate) struct WasmLoader;

impl Guest for WasmLoader {
    #[allow(async_fn_in_trait)]
    async fn load(wasm: StreamReader<u8>) -> Result<Component, Error> {
        Ok(wasm.collect().await)
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "wasm-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(WasmLoader);
