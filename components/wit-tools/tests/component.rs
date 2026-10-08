//! Runs the built `wit-tools` component in wasmtime. The component is rebuilt with
//! `make components/wit-tools` before the first test reads it, so it is never stale.
//!
//! The component is called with dynamic `Val`s rather than `bindgen!`, the `%result` record in the
//! `wit` interface generates a `Result` struct that shadows `std::result::Result` in the bindings.
//! TODO switch to `bindgen!` once https://github.com/bytecodealliance/wasmtime/pull/14543 is released.

use std::{
    path::Path,
    process::Command,
    sync::{Once, OnceLock},
};

use wasmtime::{
    Config, Engine, Store,
    component::{Component, Instance, Linker, Val},
};

const WIT_INTERFACE: &str = "componentized:component/wit@0.1.0";

/// The root of the repository, where the Makefile is.
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Rebuilds the component with make, once for every test. make skips the build when the component
/// is up to date with its sources.
fn build() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let output = Command::new("make")
            .arg("components/wit-tools")
            .current_dir(repo_root())
            .output()
            .expect("runs make");
        assert!(
            output.status.success(),
            "`make components/wit-tools` failed\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    });
}

/// A file built by `make components/wit-tools`, relative to the `target` directory.
fn built_file(path: &str) -> Vec<u8> {
    build();
    let path = repo_root().join("target").join(path);
    std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

fn component_bytes(file: &str) -> Vec<u8> {
    built_file(&format!("components/wit-tools/{file}"))
}

struct Harness {
    store: Store<()>,
    instance: Instance,
}

/// Compiles a component once, shared by every test, compiling is far slower than instantiating.
fn compile(
    compiled: &'static OnceLock<(Engine, Component)>,
    file: &str,
) -> &'static (Engine, Component) {
    compiled.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_component_model_map(true);
        let engine = Engine::new(&config).expect("engine");
        let component = Component::new(&engine, component_bytes(file)).expect("valid component");
        (engine, component)
    })
}

impl Harness {
    /// A fresh instance of a compiled component, each test has its own store.
    fn new((engine, component): &(Engine, Component)) -> Self {
        let linker = Linker::new(engine);
        let mut store = Store::new(engine, ());
        let instance = linker
            .instantiate(&mut store, component)
            .expect("instantiates");

        Self { store, instance }
    }

    fn release() -> Self {
        static COMPILED: OnceLock<(Engine, Component)> = OnceLock::new();
        Self::new(compile(&COMPILED, "wit-tools.wasm"))
    }

    fn debug() -> Self {
        static COMPILED: OnceLock<(Engine, Component)> = OnceLock::new();
        Self::new(compile(&COMPILED, "wit-tools.debug.wasm"))
    }

    /// Calls a function of the `wit` interface, returning its `result<_, error-code>`.
    fn call(&mut self, name: &str, params: &[Val]) -> wasmtime::Result<Result<Val, String>> {
        let iface = self
            .instance
            .get_export_index(&mut self.store, None, WIT_INTERFACE)
            .expect("exports the wit interface");
        let func = self
            .instance
            .get_export_index(&mut self.store, Some(&iface), name)
            .and_then(|index| self.instance.get_func(&mut self.store, index))
            .unwrap_or_else(|| panic!("exports {name}"));

        let mut results = [Val::Bool(false)];
        func.call(&mut self.store, params, &mut results)?;

        let [Val::Result(result)] = results else {
            panic!("{name} returns a result");
        };
        Ok(match result {
            Ok(value) => Ok(*value.expect("ok has a value")),
            Err(error) => Err(error_message(*error.expect("err has a value"))),
        })
    }

    fn parse(&mut self, component: &[u8]) -> Result<Val, String> {
        self.parse_source(wasm_source(component))
    }

    fn parse_source(&mut self, source: Val) -> Result<Val, String> {
        self.call("parse", &[source]).expect("parse does not trap")
    }

    fn print(&mut self, wit: Val) -> Result<String, String> {
        self.print_with_options(wit, Val::Option(None))
    }

    fn print_with_options(&mut self, wit: Val, options: Val) -> Result<String, String> {
        self.print_source(parsed_source(wit), options)
    }

    fn print_source(&mut self, source: Val, options: Val) -> Result<String, String> {
        self.call("print", &[source, options])
            .expect("print does not trap")
            .map(|printed| string(&printed).to_string())
    }

    fn summarize(&mut self, source: Val) -> Result<Val, String> {
        self.call("summarize", &[source])
            .expect("summarize does not trap")
    }
}

fn bytes(bytes: &[u8]) -> Val {
    Val::List(bytes.iter().copied().map(Val::U8).collect())
}

/// A `wit-source` of a component, or a WIT package encoded as a component.
fn wasm_source(component: &[u8]) -> Val {
    Val::Variant("wasm".to_string(), Some(Box::new(bytes(component))))
}

/// A `wit-source` of WIT text.
fn wit_source(text: &str) -> Val {
    Val::Variant(
        "wit".to_string(),
        Some(Box::new(Val::String(text.to_string()))),
    )
}

/// A `wit-source` of a `wit` from `parse`.
fn parsed_source(wit: Val) -> Val {
    Val::Variant("parsed".to_string(), Some(Box::new(wit)))
}

fn error_message(error: Val) -> String {
    match error {
        Val::Variant(case, Some(message)) if case == "other" => match *message {
            Val::Option(Some(message)) => string(&message).to_string(),
            Val::Option(None) => String::new(),
            other => panic!("unexpected error message {other:?}"),
        },
        Val::Variant(case, Some(malformed)) if case == "malformed" => {
            let message = string(field(&malformed, "message"));
            match option(field(&malformed, "name")) {
                Some(name) => format!("malformed {}: {message}", string(name)),
                None => format!("malformed: {message}"),
            }
        }
        Val::Variant(case, Some(name)) if case == "not-found" => match *name {
            Val::Option(Some(name)) => format!("not-found: {}", string(&name)),
            Val::Option(None) => "not-found".to_string(),
            other => panic!("unexpected not-found name {other:?}"),
        },
        Val::Variant(case, Some(name)) if case == "not-component" => match *name {
            Val::Option(Some(name)) => format!("not-component: {}", string(&name)),
            Val::Option(None) => "not-component".to_string(),
            other => panic!("unexpected not-component name {other:?}"),
        },
        other => panic!("unexpected error {other:?}"),
    }
}

fn field<'a>(record: &'a Val, name: &str) -> &'a Val {
    let Val::Record(fields) = record else {
        panic!("expected a record, got {record:?}");
    };
    fields
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap_or_else(|| panic!("record has field {name}"))
}

fn field_mut<'a>(record: &'a mut Val, name: &str) -> &'a mut Val {
    let Val::Record(fields) = record else {
        panic!("expected a record");
    };
    fields
        .iter_mut()
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap_or_else(|| panic!("record has field {name}"))
}

fn entry<'a>(map: &'a Val, key: &str) -> &'a Val {
    let Val::Map(entries) = map else {
        panic!("expected a map, got {map:?}");
    };
    entries
        .iter()
        .find_map(|(k, v)| (string(k) == key).then_some(v))
        .unwrap_or_else(|| panic!("map has key {key}"))
}

fn string(val: &Val) -> &str {
    match val {
        Val::String(s) => s,
        other => panic!("expected a string, got {other:?}"),
    }
}

fn strings(val: &Val) -> Vec<&str> {
    match val {
        Val::List(items) => items.iter().map(string).collect(),
        other => panic!("expected a list, got {other:?}"),
    }
}

fn option(val: &Val) -> Option<&Val> {
    match val {
        Val::Option(value) => value.as_deref(),
        other => panic!("expected an option, got {other:?}"),
    }
}

/// Prints the WIT of encoded bytes directly with `wit-component`: the decoded package, followed by
/// every other package nested, sorted by name.
fn expected_print(bytes: &[u8]) -> String {
    let decoded = wit_component::decode(bytes).expect("decodes");
    let mut nested: Vec<_> = decoded
        .resolve()
        .packages
        .iter()
        .map(|(id, _)| id)
        .filter(|id| *id != decoded.package())
        .collect();
    nested.sort_by(|a, b| {
        decoded.resolve().packages[*a]
            .name
            .cmp(&decoded.resolve().packages[*b].name)
    });
    let mut printer = wit_component::WitPrinter::default();
    printer
        .print(decoded.resolve(), decoded.package(), &nested)
        .expect("prints");
    printer.output.to_string()
}

const PACKAGE: &str = r#"
package test:integration@1.0.0;

interface shapes {
    record point { x: f64, y: f64 }
    variant shape { circle(f64), polygon(list<point>) }
    resource canvas {
        constructor();
        draw: func(shape: shape) -> result<_, string>;
    }
}

world painter {
    import shapes;
    export render: func() -> list<u8>;
}
"#;

fn wit_package(wit: &str) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::default();
    let package_id = resolve.push_str("test.wit", wit).expect("valid wit");
    wit_component::encode(&resolve, package_id, false).expect("encodes package")
}

#[test]
fn summarize_itself() {
    let mut harness = Harness::release();

    let summary = harness
        .summarize(wasm_source(&component_bytes("wit-tools.wasm")))
        .expect("summarizes");

    assert_eq!(
        strings(field(&summary, "imports")),
        vec!["componentized:component/types@0.1.0"]
    );
    assert_eq!(
        strings(field(&summary, "exports")),
        vec!["componentized:component/wit@0.1.0"]
    );
}

#[test]
fn summarize_wit_package() {
    let mut harness = Harness::release();

    for source in [wasm_source(&wit_package(PACKAGE)), wit_source(PACKAGE)] {
        let error = harness.summarize(source).expect_err("fails to summarize");

        assert_eq!(error, "not-component");
    }
}

#[test]
fn summarize_parsed() {
    let mut harness = Harness::release();
    let own_bytes = component_bytes("wit-tools.wasm");
    let wit = harness.parse(&own_bytes).expect("parses");

    let summary = harness.summarize(parsed_source(wit)).expect("summarizes");

    assert_eq!(
        strings(field(&summary, "exports")),
        vec!["componentized:component/wit@0.1.0"]
    );
}

#[test]
fn parse_itself() {
    let mut harness = Harness::release();

    let wit = harness
        .parse(&component_bytes("wit-tools.wasm"))
        .expect("parses");

    let world_id = string(option(field(&wit, "component-world")).expect("is a component"));
    let world = entry(field(&wit, "worlds"), world_id);
    assert_eq!(string(field(world, "name")), "root");

    let package = entry(
        field(&wit, "packages"),
        string(field(&wit, "default-package")),
    );
    let name = field(package, "name");
    assert_eq!(string(field(name, "namespace")), "root");
    assert_eq!(string(field(name, "name")), "component");
}

#[test]
fn parse_wit_package() {
    let mut harness = Harness::release();

    let wit = harness.parse(&wit_package(PACKAGE)).expect("parses");

    assert!(option(field(&wit, "component-world")).is_none());
    let package = entry(
        field(&wit, "packages"),
        string(field(&wit, "default-package")),
    );
    let name = field(package, "name");
    assert_eq!(string(field(name, "namespace")), "test");
    assert_eq!(string(field(name, "name")), "integration");
}

#[test]
fn parse_invalid_bytes() {
    let mut harness = Harness::release();

    let error = harness
        .parse(b"not a component")
        .expect_err("fails to parse");

    assert_eq!(error, "not-component");
}

#[test]
fn print_itself() {
    let mut harness = Harness::release();
    let own_bytes = component_bytes("wit-tools.wasm");

    let wit = harness.parse(&own_bytes).expect("parses");
    let printed = harness.print(wit).expect("prints");

    assert_eq!(printed, expected_print(&own_bytes));
    assert!(printed.starts_with("package root:component;\n"));
    assert_eq!(printed.matches("package root:component").count(), 1);
    assert!(printed.contains("package componentized:component@0.1.0 {"));
}

#[test]
fn print_wit_package() {
    let mut harness = Harness::release();
    let bytes = wit_package(PACKAGE);

    let wit = harness.parse(&bytes).expect("parses");
    let printed = harness.print(wit).expect("prints");

    assert_eq!(printed, expected_print(&bytes));
}

#[test]
fn print_unknown_type_id() {
    let mut harness = Harness::release();
    let mut wit = harness.parse(&wit_package(PACKAGE)).expect("parses");

    let Val::Map(interfaces) = field_mut(&mut wit, "interfaces") else {
        panic!("interfaces is a map");
    };
    let (_, shapes) = interfaces.first_mut().expect("has an interface");
    let Val::List(types) = field_mut(shapes, "types") else {
        panic!("types is a list");
    };
    types.push(Val::Tuple(vec![
        Val::String("missing".to_string()),
        Val::String("type:9999".to_string()),
    ]));

    let error = harness.print(wit).expect_err("fails to print");

    assert_eq!(error, "malformed: unknown type id: type:9999");
}

#[test]
fn debug_build_matches_release() {
    let own_bytes = component_bytes("wit-tools.wasm");
    let mut release = Harness::release();
    let mut debug = Harness::debug();

    let release_wit = release.parse(&own_bytes).expect("parses");
    let debug_wit = debug.parse(&own_bytes).expect("parses");

    assert_eq!(
        release.print(release_wit).expect("prints"),
        debug.print(debug_wit).expect("prints")
    );
}

/// A `package-name` without a version, selecting every version of the package.
fn package_name(namespace: &str, name: &str) -> Val {
    Val::Record(vec![
        ("namespace".to_string(), Val::String(namespace.to_string())),
        ("name".to_string(), Val::String(name.to_string())),
        ("version".to_string(), Val::Option(None)),
    ])
}

/// `option<print-options>` with `packages` set to the `printed-packages` case.
fn print_options(packages: Val) -> Val {
    Val::Option(Some(Box::new(Val::Record(vec![(
        "packages".to_string(),
        Val::Option(Some(Box::new(packages))),
    )]))))
}

#[test]
fn print_itself_without_additional_packages() {
    let mut harness = Harness::release();
    let wit = harness
        .parse(&component_bytes("wit-tools.wasm"))
        .expect("parses");

    let printed = harness
        .print_with_options(
            wit,
            print_options(Val::Variant("default".to_string(), None)),
        )
        .expect("prints");

    assert_eq!(
        printed,
        "package root:component;\n\nworld root {\n  import componentized:component/types@0.1.0;\n\n  export componentized:component/wit@0.1.0;\n}\n"
    );
}

#[test]
fn print_itself_selected_packages() {
    let mut harness = Harness::release();
    let wit = harness
        .parse(&component_bytes("wit-tools.wasm"))
        .expect("parses");

    let selected = Val::Variant(
        "selected".to_string(),
        Some(Box::new(Val::List(vec![package_name(
            "componentized",
            "component",
        )]))),
    );
    let printed = harness
        .print_with_options(wit.clone(), print_options(selected))
        .expect("prints");

    let all = harness
        .print_with_options(wit, print_options(Val::Variant("all".to_string(), None)))
        .expect("prints");
    // componentized:component is the only other package
    assert_eq!(printed, all);
    assert!(printed.contains("package componentized:component@0.1.0 {"));
}

#[test]
fn print_unknown_selected_package() {
    let mut harness = Harness::release();
    let wit = harness
        .parse(&component_bytes("wit-tools.wasm"))
        .expect("parses");

    let selected = Val::Variant(
        "selected".to_string(),
        Some(Box::new(Val::List(vec![package_name("wasi", "cli")]))),
    );
    let error = harness
        .print_with_options(wit, print_options(selected))
        .expect_err("fails to print");

    assert_eq!(error, "not-found: wasi:cli");
}

#[test]
fn parse_text() {
    let mut harness = Harness::release();

    let wit = harness.parse_source(wit_source(PACKAGE)).expect("parses");

    assert!(option(field(&wit, "component-world")).is_none());
    let package = entry(
        field(&wit, "packages"),
        string(field(&wit, "default-package")),
    );
    assert_eq!(string(field(field(package, "name"), "name")), "integration");
}

#[test]
fn parse_invalid_text() {
    let mut harness = Harness::release();

    let error = harness
        .parse_source(wit_source("package test:broken;\n\ninterface {"))
        .expect_err("fails to parse");

    assert!(error.starts_with("malformed: "), "{error}");
}

#[test]
fn print_text() {
    let mut harness = Harness::release();

    let printed = harness
        .print_source(wit_source(PACKAGE), Val::Option(None))
        .expect("prints");

    assert_eq!(printed, expected_print(&wit_package(PACKAGE)));
}

#[test]
fn print_component() {
    let mut harness = Harness::release();
    let own_bytes = component_bytes("wit-tools.wasm");

    let printed = harness
        .print_source(wasm_source(&own_bytes), Val::Option(None))
        .expect("prints");

    assert_eq!(printed, expected_print(&own_bytes));
}

#[test]
fn parse_core_module() {
    let mut harness = Harness::release();
    // the core module the component was made from, before `wasm-tools component new`
    let core_module = built_file("wasm32-unknown-unknown/release/wit_tools.wasm");

    for source in [wasm_source(&core_module)] {
        let error = harness.parse_source(source).expect_err("fails to parse");

        assert_eq!(error, "not-component");
    }
}
