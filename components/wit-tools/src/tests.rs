use std::collections::BTreeMap;

use crate::{
    WitTools,
    componentized::component::types::{ErrorCode, Malformed},
    exports::componentized::component::wit::{
        Guest, PackageName, PrintOptions, PrintedPackages, Stability, Type, TypeDefKind, Version,
        VersionIdentifier, WitSource, WorldItem, WorldKey,
    },
};

/// Exercises each kind of type, function, docs and stability a `Wit` models.
const KITCHEN_SINK: &str = r#"
/// A package with one of everything.
package test:kitchen-sink@1.2.3-alpha.1+build.5;

/// An interface with types.
@since(version = 1.0.0)
interface types {
    /// A record.
    record point {
        /// The x coordinate.
        x: s32,
        y: s32,
    }

    flags permissions {
        /// Read access.
        read,
        write,
        execute,
    }

    enum color {
        /// The color red.
        red,
        green,
        blue,
    }

    variant shape {
        /// A circle with a radius.
        circle(f64),
        square(f64),
        empty,
    }

    type alias = point;
    type pair = tuple<u8, u16, u32, u64>;
    type maybe = option<s8>;
    type fallible = result<s16, string>;
    type ok-only = result<s64>;
    type err-only = result<_, char>;
    type empty-result = result;
    type points = list<point>;
    type quad = list<f32, 4>;
    type lookup = map<string, bool>;
    type values = stream<u8>;
    type unit-stream = stream;
    type pending = future<string>;
    type unit-future = future;

    /// A resource.
    resource counter {
        /// Creates a counter.
        constructor(start: u32);
        increment: func();
        get: func() -> u32;
        merge: static func(a: borrow<counter>, b: own<counter>) -> counter;
        wait: async func();
    }

    @unstable(feature = experimental)
    unstable-func: func();

    @since(version = 1.0.0)
    @deprecated(version = 1.1.0)
    deprecated-func: func();

    /// A free function.
    distance: func(a: point, b: point) -> f64;
    sleep: async func(ms: u64);
}

interface consumer {
    use types.{point, counter};

    centroid: func(points: list<point>) -> point;
    total: func(c: borrow<counter>) -> u32;
}

/// A world.
world app {
    import types;
    import log: func(message: string);
    import host: interface {
        now: func() -> u64;
    }

    use types.{point};
    type local-point = point;

    export consumer;
    export run: func(p: local-point);
}

world extended {
    include app with { log as logger }
    export extra: func();
}
"#;

const SIMPLE: &str = r#"
package test:simple@0.1.0;

interface greeter {
    greet: func(name: string) -> string;
}

world hello {
    import greeter;
    export greeter;
    export run: func();
}
"#;

fn resolve(wit: &str) -> (wit_parser::Resolve, wit_parser::PackageId) {
    resolve_with_deps(&[], wit)
}

/// Resolves `wit` after each of the `deps` packages it uses.
fn resolve_with_deps(deps: &[&str], wit: &str) -> (wit_parser::Resolve, wit_parser::PackageId) {
    let mut resolve = wit_parser::Resolve::default();
    resolve.all_features = true;
    for (i, dep) in deps.iter().enumerate() {
        resolve
            .push_str(format!("dep{i}.wit"), dep)
            .expect("valid dependency wit");
    }
    let package_id = resolve.push_str("test.wit", wit).expect("valid wit");
    (resolve, package_id)
}

/// Encodes a WIT package as a binary package.
fn wit_package(wit: &str) -> Vec<u8> {
    let (resolve, package_id) = resolve(wit);
    wit_component::encode(&resolve, package_id, false).expect("encodes package")
}

/// Encodes a component targeting the named world, with dummy implementations of its exports.
fn component(wit: &str, world: &str) -> Vec<u8> {
    component_with_deps(&[], wit, world)
}

/// Encodes a component targeting the named world of `wit`, which uses the `deps` packages.
fn component_with_deps(deps: &[&str], wit: &str, world: &str) -> Vec<u8> {
    let (resolve, package_id) = resolve_with_deps(deps, wit);
    let world = resolve
        .select_world(&[package_id], Some(world))
        .expect("world exists");
    let mut module =
        wit_component::dummy_module(&resolve, world, wit_parser::ManglingAndAbi::Standard32);
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
        false,
    )
    .expect("embeds metadata");
    wit_component::ComponentEncoder::default()
        .module(&module)
        .expect("valid module")
        .validate(true)
        .encode()
        .expect("encodes component")
}

/// Prints the WIT of encoded bytes directly with `wit-component`, the output `print` should match:
/// the decoded package, followed by every other package nested, sorted by name.
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

/// The message of a `malformed` error for the single source of a call.
fn malformed_message(error: ErrorCode) -> String {
    match error {
        ErrorCode::Malformed(Malformed {
            name: None,
            message,
        }) => message,
        error => panic!("expected a malformed error without a name, got {error:?}"),
    }
}

#[test]
fn parse_wit_package() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(SIMPLE))).expect("parses");

    assert_eq!(wit.component_world, None);

    let package = &wit.packages[&wit.default_package];
    assert_eq!(package.name.namespace, "test");
    assert_eq!(package.name.name, "simple");
    assert_eq!(
        package.name.version.as_ref().map(ToString::to_string),
        Some("0.1.0".to_string())
    );
    assert_eq!(
        package
            .interfaces
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["greeter"]
    );
    assert_eq!(
        package
            .worlds
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["hello"]
    );

    let (_, greeter_id) = &package.interfaces[0];
    let greeter = &wit.interfaces[greeter_id];
    assert_eq!(greeter.name.as_deref(), Some("greeter"));
    let (name, greet) = &greeter.functions[0];
    assert_eq!(name, "greet");
    assert_eq!(greet.params.len(), 1);
    assert_eq!(greet.params[0].name, "name");
    assert!(matches!(greet.params[0].type_, Type::String));
    assert!(matches!(greet.result, Some(Type::String)));
}

#[test]
fn parse_component() {
    let wit = WitTools::parse(WitSource::Wasm(component(SIMPLE, "hello"))).expect("parses");

    let world_id = wit.component_world.clone().expect("has a component world");
    let world = &wit.worlds[&world_id];
    assert_eq!(world.imports.len(), 1);
    assert!(matches!(
        &world.imports[0],
        (WorldKey::Interface(_), WorldItem::Interface(_))
    ));
    assert_eq!(world.exports.len(), 2);
    assert!(world.exports.iter().any(
        |(key, item)| matches!(key, WorldKey::Name(name) if name == "run")
            && matches!(item, WorldItem::Function(_))
    ));
}

#[test]
fn parse_ids_are_prefixed_by_kind() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");

    assert!(wit.packages.keys().all(|id| id.starts_with("package:")));
    assert!(wit.interfaces.keys().all(|id| id.starts_with("interface:")));
    assert!(wit.types.keys().all(|id| id.starts_with("type:")));
    assert!(wit.worlds.keys().all(|id| id.starts_with("world:")));
}

#[test]
fn parse_type_def_kinds() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");

    let kind = |name: &str| {
        wit.types
            .values()
            .find(|type_def| type_def.name.as_deref() == Some(name))
            .map(|type_def| &type_def.kind)
            .unwrap_or_else(|| panic!("type {name} exists"))
    };

    assert!(matches!(kind("point"), TypeDefKind::Record(r) if r.fields.len() == 2));
    assert!(matches!(kind("permissions"), TypeDefKind::Flags(f) if f.flags.len() == 3));
    assert!(matches!(kind("color"), TypeDefKind::Enum(e) if e.cases.len() == 3));
    assert!(matches!(kind("shape"), TypeDefKind::Variant(v) if v.cases.len() == 3));
    assert!(matches!(kind("alias"), TypeDefKind::Type(Type::Id(_))));
    assert!(matches!(kind("pair"), TypeDefKind::Tuple(t) if t.types.len() == 4));
    assert!(matches!(kind("maybe"), TypeDefKind::Option(Type::S8)));
    assert!(matches!(
        kind("fallible"),
        TypeDefKind::Result(r) if matches!(r.ok, Some(Type::S16)) && matches!(r.err, Some(Type::String))
    ));
    assert!(matches!(
        kind("empty-result"),
        TypeDefKind::Result(r) if r.ok.is_none() && r.err.is_none()
    ));
    assert!(matches!(kind("points"), TypeDefKind::List(l) if l.fixed_length.is_none()));
    assert!(matches!(
        kind("quad"),
        TypeDefKind::List(l) if l.fixed_length == Some(4) && matches!(l.type_, Type::F32)
    ));
    assert!(matches!(
        kind("lookup"),
        TypeDefKind::Map(m) if matches!(m.key, Type::String) && matches!(m.value, Type::Bool)
    ));
    assert!(matches!(
        kind("values"),
        TypeDefKind::Stream(Some(Type::U8))
    ));
    assert!(matches!(kind("unit-stream"), TypeDefKind::Stream(None)));
    assert!(matches!(
        kind("pending"),
        TypeDefKind::Future(Some(Type::String))
    ));
    assert!(matches!(kind("unit-future"), TypeDefKind::Future(None)));
    assert!(matches!(kind("counter"), TypeDefKind::Resource));
}

#[test]
fn parse_docs_and_stability() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");

    let package = &wit.packages[&wit.default_package];
    assert_eq!(
        package.docs.contents.as_deref(),
        Some("A package with one of everything.")
    );

    let types = wit
        .interfaces
        .values()
        .find(|iface| iface.name.as_deref() == Some("types"))
        .expect("types interface");
    assert_eq!(
        types.docs.contents.as_deref(),
        Some("An interface with types.")
    );
    assert!(matches!(
        &types.stability,
        Stability::Stable(stable) if stable.since.to_string() == "1.0.0" && stable.deprecated.is_none()
    ));

    let function = |name: &str| {
        &types
            .functions
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("function {name} exists"))
            .1
    };
    assert!(matches!(
        &function("unstable-func").stability,
        Stability::Unstable(unstable) if unstable.feature == "experimental"
    ));
    assert!(matches!(
        &function("deprecated-func").stability,
        Stability::Stable(stable)
            if stable.deprecated.as_ref().map(ToString::to_string).as_deref() == Some("1.1.0")
    ));
    assert_eq!(
        function("distance").docs.contents.as_deref(),
        Some("A free function.")
    );
}

#[test]
fn parse_invalid_bytes() {
    let error =
        WitTools::parse(WitSource::Wasm(b"not a component".to_vec())).expect_err("fails to parse");
    assert!(matches!(error, ErrorCode::NotComponent(None)));
}

#[test]
fn parse_invalid_component() {
    // a component header followed by an invalid section
    let mut bytes = wit_package(SIMPLE)[..8].to_vec();
    bytes.extend([0xff, 0xff]);

    let error = WitTools::parse(WitSource::Wasm(bytes)).expect_err("fails to parse");
    assert!(!malformed_message(error).is_empty());
}

#[test]
fn print_wit_package() {
    let bytes = wit_package(SIMPLE);
    let wit = WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses");
    assert_eq!(
        WitTools::print(WitSource::Parsed(wit), None).expect("prints"),
        expected_print(&bytes)
    );
}

#[test]
fn print_kitchen_sink() {
    let bytes = wit_package(KITCHEN_SINK);
    let wit = WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses");
    let printed = WitTools::print(WitSource::Parsed(wit), None).expect("prints");

    assert_eq!(printed, expected_print(&bytes));
    assert!(printed.starts_with(
        "/// A package with one of everything.\npackage test:kitchen-sink@1.2.3-alpha.1+build.5;"
    ));
}

#[test]
fn print_component() {
    let bytes = component(KITCHEN_SINK, "app");
    let wit = WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses");
    assert_eq!(
        WitTools::print(WitSource::Parsed(wit), None).expect("prints"),
        expected_print(&bytes)
    );
}

const ZEBRA: &str = r#"
package test:zebra@2.0.0;

interface stripes {
    count: func() -> u32;
}
"#;

const APPLE: &str = r#"
package test:apple@1.0.0;

interface seeds {
    count: func() -> u32;
}
"#;

const ORCHARD: &str = r#"
package test:orchard;

world orchard {
    import test:zebra/stripes@2.0.0;
    import test:apple/seeds@1.0.0;
    export run: func();
}
"#;

#[test]
fn print_nests_other_packages_sorted_by_name() {
    let bytes = component_with_deps(&[ZEBRA, APPLE], ORCHARD, "orchard");
    let wit = WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses");
    let printed = WitTools::print(WitSource::Parsed(wit), None).expect("prints");

    assert_eq!(printed, expected_print(&bytes));

    // the default package is printed first and only once
    assert!(printed.starts_with("package root:component;\n"));
    assert_eq!(printed.matches("package root:component").count(), 1);

    // every other package is nested, sorted by name regardless of the order they were imported
    let apple = printed
        .find("package test:apple@1.0.0 {")
        .expect("nests test:apple");
    let zebra = printed
        .find("package test:zebra@2.0.0 {")
        .expect("nests test:zebra");
    assert!(apple < zebra, "test:apple is nested before test:zebra");
}

#[test]
fn print_wit_package_has_no_nested_packages() {
    let printed = WitTools::print(
        WitSource::Parsed(WitTools::parse(WitSource::Wasm(wit_package(SIMPLE))).expect("parses")),
        None,
    )
    .expect("prints");

    assert_eq!(printed.matches("package ").count(), 1);
}

#[test]
fn print_component_is_reparseable() {
    let bytes = component_with_deps(&[ZEBRA, APPLE], ORCHARD, "orchard");
    let printed = WitTools::print(
        WitSource::Parsed(WitTools::parse(WitSource::Wasm(bytes)).expect("parses")),
        None,
    )
    .expect("prints");

    let reprinted = WitTools::print(
        WitSource::Parsed(WitTools::parse(WitSource::Wasm(wit_package(&printed))).expect("parses")),
        None,
    )
    .expect("prints");
    assert_eq!(printed, reprinted);
}

#[test]
fn print_is_reparseable() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");
    let printed = WitTools::print(WitSource::Parsed(wit), None).expect("prints");

    let reprinted = WitTools::print(
        WitSource::Parsed(WitTools::parse(WitSource::Wasm(wit_package(&printed))).expect("parses")),
        None,
    )
    .expect("prints");
    assert_eq!(printed, reprinted);
}

#[test]
fn print_unknown_type_id() {
    let mut wit = WitTools::parse(WitSource::Wasm(wit_package(SIMPLE))).expect("parses");
    let (_, iface_id) = wit.packages[&wit.default_package].interfaces[0].clone();
    let iface = wit.interfaces.get_mut(&iface_id).expect("interface exists");
    iface
        .types
        .push(("missing".to_string(), "type:9999".to_string()));

    let error = WitTools::print(WitSource::Parsed(wit), None).expect_err("fails to print");
    assert_eq!(malformed_message(error), "unknown type id: type:9999");
}

#[test]
fn print_unknown_default_package() {
    let mut wit = WitTools::parse(WitSource::Wasm(wit_package(SIMPLE))).expect("parses");
    wit.default_package = "package:9999".to_string();

    let error = WitTools::print(WitSource::Parsed(wit), None).expect_err("fails to print");
    assert_eq!(malformed_message(error), "unknown package id: package:9999");
}

#[test]
fn print_invalid_version() {
    let mut wit = WitTools::parse(WitSource::Wasm(wit_package(SIMPLE))).expect("parses");
    let package = wit
        .packages
        .get_mut(&wit.default_package.clone())
        .expect("package exists");
    package.name.version = Some(Version {
        major: 1,
        minor: 0,
        patch: 0,
        prerelease: Some(vec![VersionIdentifier::String("not valid".to_string())]),
        build_metadata: None,
    });

    let error = WitTools::print(WitSource::Parsed(wit), None).expect_err("fails to print");
    assert!(
        malformed_message(error).starts_with("invalid version 1.0.0-not valid:"),
        "unexpected error"
    );
}

#[test]
fn print_preserves_order_of_ids_beyond_nine() {
    // ids sort numerically, not lexically, `type:10` follows `type:9`
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");
    assert!(wit.types.len() > 10, "needs more than ten types");

    let bytes = wit_package(KITCHEN_SINK);
    assert_eq!(
        WitTools::print(WitSource::Parsed(wit), None).expect("prints"),
        expected_print(&bytes)
    );
}

#[test]
fn resolve_ids_assign_orders_numerically() {
    let arena = id_arena::Arena::<()>::new();
    let keys = ["type:10", "type:2", "type:0", "type:1"].map(String::from);

    let ids = crate::ResolveIds::assign(keys.iter(), &arena);

    let indexes: BTreeMap<&str, usize> = ids
        .iter()
        .map(|(key, id)| (key.as_str(), id.index()))
        .collect();
    assert_eq!(
        indexes,
        BTreeMap::from([("type:0", 0), ("type:1", 1), ("type:2", 2), ("type:10", 3)])
    );
}

#[test]
fn summarize() {
    let summary =
        WitTools::summarize(WitSource::Wasm(component(KITCHEN_SINK, "app"))).expect("summarizes");

    assert_eq!(
        summary.imports,
        vec![
            "test:kitchen-sink/types@1.2.3-alpha.1+build.5",
            "host",
            // `use types.{point}`, `type local-point` and `log` are not interfaces
            "--unknown--",
            "--unknown--",
            "--unknown--",
        ]
    );
    assert_eq!(
        summary.exports,
        vec![
            // `run` is a function, not an interface
            "--unknown--",
            "test:kitchen-sink/consumer@1.2.3-alpha.1+build.5",
        ]
    );
}

#[test]
fn version_display() {
    let version = Version {
        major: 1,
        minor: 2,
        patch: 3,
        prerelease: Some(vec![
            VersionIdentifier::String("alpha".to_string()),
            VersionIdentifier::Numeric(1),
        ]),
        build_metadata: Some(vec![
            VersionIdentifier::String("build".to_string()),
            VersionIdentifier::Numeric(5),
        ]),
    };
    assert_eq!(version.to_string(), "1.2.3-alpha.1+build.5");

    let version = Version {
        major: 0,
        minor: 1,
        patch: 0,
        prerelease: None,
        build_metadata: None,
    };
    assert_eq!(version.to_string(), "0.1.0");
}

fn print_orchard(packages: Option<PrintedPackages>) -> Result<String, ErrorCode> {
    let bytes = component_with_deps(&[ZEBRA, APPLE], ORCHARD, "orchard");
    let wit = WitTools::parse(WitSource::Wasm(bytes)).expect("parses");
    WitTools::print(WitSource::Parsed(wit), Some(PrintOptions { packages }))
}

/// A package name from `namespace:name` or `namespace:name@version`.
fn package_name(name: &str) -> PackageName {
    let (name, version) = match name.split_once('@') {
        Some((name, version)) => (name, Some(version)),
        None => (name, None),
    };
    let (namespace, name) = name.split_once(':').expect("namespace:name");
    PackageName {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: version
            .map(|version| crate::Wit::version(semver::Version::parse(version).expect("semver"))),
    }
}

fn nested_packages(printed: &str) -> Vec<&str> {
    printed
        .lines()
        .filter_map(|line| line.strip_prefix("package ")?.strip_suffix(" {"))
        .collect()
}

#[test]
fn print_options_default_to_all_packages() {
    let without_options = {
        let bytes = component_with_deps(&[ZEBRA, APPLE], ORCHARD, "orchard");
        WitTools::print(
            WitSource::Parsed(WitTools::parse(WitSource::Wasm(bytes)).expect("parses")),
            None,
        )
        .expect("prints")
    };
    let all = print_orchard(Some(PrintedPackages::All)).expect("prints");
    let unset = print_orchard(None).expect("prints");

    assert_eq!(
        nested_packages(&all),
        vec!["test:apple@1.0.0", "test:zebra@2.0.0"]
    );
    assert_eq!(without_options, all);
    assert_eq!(unset, all);
}

#[test]
fn print_no_additional_packages() {
    let printed = print_orchard(Some(PrintedPackages::Default)).expect("prints");

    assert!(printed.starts_with("package root:component;\n"));
    assert_eq!(nested_packages(&printed), Vec::<&str>::new());
}

#[test]
fn print_selected_packages() {
    let printed = print_orchard(Some(PrintedPackages::Selected(vec![package_name(
        "test:zebra@2.0.0",
    )])))
    .expect("prints");

    assert_eq!(nested_packages(&printed), vec!["test:zebra@2.0.0"]);
}

#[test]
fn print_selected_packages_sorted_by_name() {
    let printed = print_orchard(Some(PrintedPackages::Selected(vec![
        package_name("test:zebra@2.0.0"),
        package_name("test:apple@1.0.0"),
    ])))
    .expect("prints");

    assert_eq!(
        nested_packages(&printed),
        vec!["test:apple@1.0.0", "test:zebra@2.0.0"]
    );
}

#[test]
fn print_selected_package_without_version() {
    let printed = print_orchard(Some(PrintedPackages::Selected(vec![package_name(
        "test:apple",
    )])))
    .expect("prints");

    assert_eq!(nested_packages(&printed), vec!["test:apple@1.0.0"]);
}

#[test]
fn print_selected_package_once() {
    let printed = print_orchard(Some(PrintedPackages::Selected(vec![
        package_name("test:apple"),
        package_name("test:apple@1.0.0"),
        // the default package is never nested
        package_name("root:component"),
    ])))
    .expect("prints");

    assert_eq!(nested_packages(&printed), vec!["test:apple@1.0.0"]);
    assert_eq!(printed.matches("package root:component").count(), 1);
}

#[test]
fn print_selected_unknown_package() {
    let error = print_orchard(Some(PrintedPackages::Selected(vec![package_name(
        "test:apple@9.9.9",
    )])))
    .expect_err("fails to print");

    assert!(matches!(error, ErrorCode::NotFound(Some(name)) if name == "test:apple@9.9.9"));
}

#[test]
fn parse_text() {
    let wit = WitTools::parse(WitSource::Wit(SIMPLE.to_string())).expect("parses");

    assert_eq!(wit.component_world, None);
    let package = &wit.packages[&wit.default_package];
    assert_eq!(package.name.to_string(), "test:simple@0.1.0");
}

#[test]
fn parse_invalid_text() {
    let error = WitTools::parse(WitSource::Wit(
        "package test:broken;\n\ninterface {".to_string(),
    ))
    .expect_err("fails to parse");

    assert!(!malformed_message(error).is_empty());
}

#[test]
fn parse_text_with_unknown_package() {
    // packages the text uses must be nested within it
    let error = WitTools::parse(WitSource::Wit(ORCHARD.to_string())).expect_err("fails to parse");

    assert!(malformed_message(error).contains("test:zebra"));
}

#[test]
fn print_text_matches_encoded_package() {
    let from_text =
        WitTools::print(WitSource::Wit(KITCHEN_SINK.to_string()), None).expect("prints");
    let from_component =
        WitTools::print(WitSource::Wasm(wit_package(KITCHEN_SINK)), None).expect("prints");

    assert_eq!(from_text, from_component);
    // `@unstable` items are kept
    assert!(from_text.contains("unstable-func: func();"));
}

#[test]
fn print_text_with_nested_packages() {
    let text = format!(
        "{ORCHARD}\n{}\n{}",
        nest(ZEBRA, "test:zebra@2.0.0"),
        nest(APPLE, "test:apple@1.0.0")
    );

    let printed = WitTools::print(WitSource::Wit(text), None).expect("prints");

    assert!(printed.starts_with("package test:orchard;\n"));
    assert_eq!(
        nested_packages(&printed),
        vec!["test:apple@1.0.0", "test:zebra@2.0.0"]
    );
}

/// Rewrites a package as nested package syntax, `package name { ... }`.
fn nest(wit: &str, name: &str) -> String {
    let body = wit.replace(&format!("package {name};"), "");
    format!("package {name} {{\n{body}\n}}")
}

#[test]
fn print_component_matches_parsed() {
    let bytes = component(KITCHEN_SINK, "app");

    let from_component = WitTools::print(WitSource::Wasm(bytes.clone()), None).expect("prints");
    let from_parsed = WitTools::print(
        WitSource::Parsed(WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses")),
        None,
    )
    .expect("prints");

    assert_eq!(from_component, expected_print(&bytes));
    assert_eq!(from_component, from_parsed);
}

#[test]
fn parse_parsed_is_unchanged() {
    let wit = WitTools::parse(WitSource::Wasm(wit_package(KITCHEN_SINK))).expect("parses");
    let printed = WitTools::print(WitSource::Parsed(wit.clone()), None).expect("prints");

    let reparsed = WitTools::parse(WitSource::Parsed(wit)).expect("parses");

    assert_eq!(
        WitTools::print(WitSource::Parsed(reparsed), None).expect("prints"),
        printed
    );
}

#[test]
fn summarize_parsed() {
    let bytes = component(KITCHEN_SINK, "app");
    let wit = WitTools::parse(WitSource::Wasm(bytes.clone())).expect("parses");

    let from_parsed = WitTools::summarize(WitSource::Parsed(wit)).expect("summarizes");
    let from_component = WitTools::summarize(WitSource::Wasm(bytes)).expect("summarizes");

    assert_eq!(from_parsed.imports, from_component.imports);
    assert_eq!(from_parsed.exports, from_component.exports);
}

#[test]
fn summarize_wit_package() {
    for source in [
        WitSource::Wasm(wit_package(SIMPLE)),
        WitSource::Wit(SIMPLE.to_string()),
    ] {
        let error = WitTools::summarize(source).expect_err("fails to summarize");

        assert!(matches!(error, ErrorCode::NotComponent(None)));
    }
}

#[test]
fn summarize_unknown_component_world() {
    let mut wit = WitTools::parse(WitSource::Wasm(component(SIMPLE, "hello"))).expect("parses");
    wit.component_world = Some("world:9999".to_string());

    let error = WitTools::summarize(WitSource::Parsed(wit)).expect_err("fails to summarize");

    assert_eq!(malformed_message(error), "unknown world id: world:9999");
}

#[test]
fn parse_core_module() {
    let source = WitSource::Wasm(wat::parse_str("(module)").expect("valid wat"));
    let error = WitTools::parse(source).expect_err("fails to parse");

    assert!(matches!(error, ErrorCode::NotComponent(None)));
}

#[test]
fn core_module_with_component_metadata() {
    // a module embedding component metadata, as built by `wit-bindgen`, before `wasm-tools
    // component new`
    let (resolve, package_id) = resolve(SIMPLE);
    let world = resolve
        .select_world(&[package_id], Some("hello"))
        .expect("world exists");
    let mut module =
        wit_component::dummy_module(&resolve, world, wit_parser::ManglingAndAbi::Standard32);
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
        false,
    )
    .expect("embeds metadata");

    let error = WitTools::print(WitSource::Wasm(module), None).expect_err("fails to print");
    assert!(matches!(error, ErrorCode::NotComponent(None)));
}
