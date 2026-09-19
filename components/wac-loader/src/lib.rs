#![no_main]

use indexmap::IndexMap;
use wac_graph::{
    types::{BorrowedPackageKey, Package},
    CompositionGraph, EncodeOptions,
};
use wac_parser::Document;

use crate::exports::componentized::component::{
    types::{Component, Error},
    wac_loader::{Dependency, Guest, Plan},
};

pub(crate) struct WacLoader;

impl Guest for WacLoader {
    #[allow(async_fn_in_trait)]
    async fn plug(socket: Component, plugs: Vec<Component>) -> Result<Component, Error> {
        let mut graph = CompositionGraph::new();

        let socket = Package::from_bytes("socket", None, socket, graph.types_mut())?;
        let socket = graph.register_package(socket)?;

        let mut graph_plugs = Vec::new();
        for (i, plug) in plugs.into_iter().enumerate() {
            let plug = Package::from_bytes(&format!("plug:{i}"), None, plug, graph.types_mut())?;
            let plug = graph.register_package(plug)?;
            graph_plugs.push(plug);
        }

        wac_graph::plug(&mut graph, graph_plugs, socket)?;
        let component = graph.encode(EncodeOptions::default())?;

        Ok(component)
    }

    #[allow(async_fn_in_trait)]
    async fn compose(plan: Plan, deps: Vec<Dependency>) -> Result<Component, Error> {
        match plan {
            Plan::Wac(script) => {
                let document = Document::parse(&script)?;

                let (names, components): (Vec<String>, Vec<Component>) = deps.into_iter().unzip();
                let mut dependencies = IndexMap::new();
                for (pkg, component) in names.iter().zip(components) {
                    let key = BorrowedPackageKey::from_name_and_version(pkg, None);
                    dependencies.insert(key, component);
                }
                let resolution = document.resolve(dependencies)?;
                let component = resolution.encode(EncodeOptions::default())?;

                Ok(component)
            }
        }
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

impl From<wac_parser::Error> for Error {
    fn from(value: wac_parser::Error) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

impl From<wac_parser::resolution::Error> for Error {
    fn from(value: wac_parser::resolution::Error) -> Self {
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
