use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};

use crate::{
    WacLoader,
    componentized::component::types::{ErrorCode, Malformed},
    exports::componentized::component::wac_loader::{Guest, Plan},
};

/// Polls a future that completes without waiting, the loader does not await I/O.
fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("future is not ready"),
    }
}

fn empty_component() -> Vec<u8> {
    wat::parse_str("(component)").expect("valid wat")
}

fn core_module() -> Vec<u8> {
    wat::parse_str("(module)").expect("valid wat")
}

#[test]
fn plug_socket_not_component() {
    let error =
        ready(WacLoader::plug(core_module(), vec![empty_component()])).expect_err("fails to plug");

    assert!(matches!(error, ErrorCode::NotComponent(Some(name)) if name == "socket"));
}

#[test]
fn plug_names_plug_not_component() {
    let error = ready(WacLoader::plug(
        empty_component(),
        vec![empty_component(), core_module()],
    ))
    .expect_err("fails to plug");

    assert!(matches!(error, ErrorCode::NotComponent(Some(name)) if name == "plug:1"));
}

#[test]
fn plug_bytes_not_component() {
    let error = ready(WacLoader::plug(b"not wasm".to_vec(), vec![])).expect_err("fails to plug");

    assert!(matches!(error, ErrorCode::NotComponent(Some(name)) if name == "socket"));
}

#[test]
fn compose_names_dependency_not_component() {
    let error = ready(WacLoader::compose(
        Plan::Wac("package test:composition;".to_string()),
        vec![
            ("test:ok".to_string(), empty_component()),
            ("test:core".to_string(), core_module()),
        ],
    ))
    .expect_err("fails to compose");

    assert!(matches!(error, ErrorCode::NotComponent(Some(name)) if name == "test:core"));
}

#[test]
fn compose_components() {
    let composed = ready(WacLoader::compose(
        Plan::Wac("package test:composition;".to_string()),
        vec![("test:ok".to_string(), empty_component())],
    ))
    .expect("composes");

    assert!(wasmparser::Parser::is_component(&composed));
}

/// A component header followed by an invalid section.
fn malformed_component() -> Vec<u8> {
    let mut bytes = empty_component()[..8].to_vec();
    bytes.extend([0xff, 0xff]);
    bytes
}

#[test]
fn plug_names_malformed_plug() {
    let error = ready(WacLoader::plug(
        empty_component(),
        vec![malformed_component()],
    ))
    .expect_err("fails to plug");

    assert!(matches!(
        error,
        ErrorCode::Malformed(Malformed { name: Some(name), message })
            if name == "plug:0" && !message.is_empty()
    ));
}

#[test]
fn compose_names_malformed_dependency() {
    let error = ready(WacLoader::compose(
        Plan::Wac("package test:composition;".to_string()),
        vec![("test:broken".to_string(), malformed_component())],
    ))
    .expect_err("fails to compose");

    assert!(matches!(
        error,
        ErrorCode::Malformed(Malformed { name: Some(name), .. }) if name == "test:broken"
    ));
}

#[test]
fn compose_unknown_package() {
    let error = ready(WacLoader::compose(
        Plan::Wac("package test:composition;\n\nlet missing = new test:missing {};".to_string()),
        vec![("test:ok".to_string(), empty_component())],
    ))
    .expect_err("fails to compose");

    assert!(matches!(error, ErrorCode::NotFound(Some(name)) if name == "test:missing"));
}
