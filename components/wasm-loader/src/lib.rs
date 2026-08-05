#![no_main]

use crate::{
    exports::componentized::component::types::{
        Component, Error, Guest as TypesGuest, GuestComponent,
    },
    exports::componentized::component::wasm_loader::Guest,
};
use wit_bindgen::rt::async_support::StreamReader;

pub(crate) struct WasmLoader;

impl TypesGuest for WasmLoader {
    type Component = WasmComponent;
}

impl Guest for WasmLoader {
    #[allow(async_fn_in_trait)]
    async fn load(wasm: StreamReader<u8>) -> Result<Component, Error> {
        let wasm = wasm.collect().await;
        Ok(Component::new(WasmComponent::new(wasm)))
    }
}

pub(crate) struct WasmComponent {
    wasm: Vec<u8>,
}

impl WasmComponent {
    fn new(wasm: Vec<u8>) -> Self {
        Self { wasm }
    }
}

impl GuestComponent for WasmComponent {
    #[allow(async_fn_in_trait)]
    fn into_wasm(&self) -> Vec<u8> {
        self.wasm.clone()
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "wasm-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(WasmLoader);
