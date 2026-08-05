#![no_main]

use wac_graph::{types::Package, CompositionGraph, EncodeOptions};

use crate::exports::componentized::component::{
    types::{Component, ComponentBorrow, Error, Guest as TypesGuest, GuestComponent},
    wac_loader::Guest,
};

pub(crate) struct WacLoader;

impl TypesGuest for WacLoader {
    type Component = WacComponent;
}

impl Guest for WacLoader {
    #[allow(async_fn_in_trait)]
    async fn plug(
        socket: ComponentBorrow<'_>,
        plugs: Vec<ComponentBorrow<'_>>,
    ) -> Result<Component, Error> {
        let mut graph = CompositionGraph::new();

        let socket: &WacComponent = socket.get();
        let socket = Package::from_bytes("socket", None, socket.into_wasm(), graph.types_mut())?;
        let socket = graph.register_package(socket)?;

        let mut graph_plugs = Vec::new();
        for plug in plugs {
            let plug: &WacComponent = plug.get();
            let plug = Package::from_bytes("plug", None, plug.into_wasm(), graph.types_mut())?;
            let plug = graph.register_package(plug)?;
            graph_plugs.push(plug);
        }

        wac_graph::plug(&mut graph, graph_plugs, socket)?;
        let composed_wasm = graph.encode(EncodeOptions::default())?;

        Ok(Component::new(WacComponent::new(composed_wasm)))
    }
}

pub(crate) struct WacComponent {
    wasm: Vec<u8>,
}

impl WacComponent {
    fn new(wasm: Vec<u8>) -> Self {
        Self { wasm }
    }
}

impl GuestComponent for WacComponent {
    #[allow(async_fn_in_trait)]
    fn into_wasm(&self) -> Vec<u8> {
        self.wasm.clone()
    }
}

impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

impl From<wac_graph::EncodeError> for Error {
    fn from(value: wac_graph::EncodeError) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

impl From<wac_graph::PlugError> for Error {
    fn from(value: wac_graph::PlugError) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

impl From<wac_graph::RegisterPackageError> for Error {
    fn from(value: wac_graph::RegisterPackageError) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "wac-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(WacLoader);
