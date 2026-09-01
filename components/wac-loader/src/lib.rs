#![no_main]

use wac_graph::{types::Package, CompositionGraph, EncodeOptions};

use crate::exports::componentized::component::{
    types::{Component, Error},
    wac_loader::Guest,
};

pub(crate) struct WacLoader;

impl Guest for WacLoader {
    #[allow(async_fn_in_trait)]
    async fn plug(socket: Component, plugs: Vec<Component>) -> Result<Component, Error> {
        let mut graph = CompositionGraph::new();

        let socket = Package::from_bytes("socket", None, socket.bytes, graph.types_mut())?;
        let socket = graph.register_package(socket)?;

        let mut graph_plugs = Vec::new();
        for plug in plugs {
            let plug = Package::from_bytes("plug", None, plug.bytes, graph.types_mut())?;
            let plug = graph.register_package(plug)?;
            graph_plugs.push(plug);
        }

        wac_graph::plug(&mut graph, graph_plugs, socket)?;
        let composed_wasm = graph.encode(EncodeOptions::default())?;

        Ok(Component {
            bytes: composed_wasm,
        })
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
