//! `u64be` is a real wire type: it parses, validates, and emits big-endian
//! on every language surface that already emits `u64le`, and a C-CMIS
//! Page 34h counter decodes MSB-first.

use openxcvr_codegen::emitters::rust::pages::emit_page as emit_rust_page;
use openxcvr_codegen::emitters::{
    csharp::CSharpEmitter, go::GoEmitter, java::JavaEmitter, openapi::OpenApiEmitter,
    php::PhpEmitter, python::PythonEmitter, typescript::TypeScriptEmitter, Emitter,
};
use openxcvr_codegen::ir::pages::FieldType;
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};
use openxcvr_codegen::validator::validate_pages;
use std::path::PathBuf;
use std::process::Command;

fn schemas_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

const COUNTER_BYTES: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02];

/// MSB at the lowest address, from the specification's byte-order rule,
/// not from a decoder.
fn msb_first(bytes: &[u8; 8]) -> u64 {
    bytes
        .iter()
        .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte))
}

fn round_trip_page() -> openxcvr_codegen::ir::pages::PageSchema {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: U64BeRoundTrip
title: u64be round trip
page_address: lower
size_bytes: 8
fields:
  - { name: counter, offset: 0, size_bytes: 8, type: u64be, access: rw }
"#;
    let schema = parse_page_schema(yaml).expect("u64be parses");
    validate_pages(std::slice::from_ref(&schema)).expect("u64be validates");
    let field = schema
        .fields
        .iter()
        .find(|field| field.name == "counter")
        .expect("counter field");
    assert_eq!(field.ty, FieldType::U64Be);
    assert_eq!(field.ty.wire_name(), "u64be");
    assert_eq!(field.ty.rust_type(None).as_deref(), Some("u64"));
    assert_eq!(field.effective_bit_width(), 64);
    schema
}

#[test]
fn u64be_round_trips_through_parse_validate_and_emit() {
    let schema = round_trip_page();
    let rust = emit_rust_page(&schema);
    assert!(
        rust.contains("counter: u64::from_be_bytes(["),
        "rust decode must be big-endian:\n{rust}"
    );
    assert!(
        rust.contains("copy_from_slice(&value.to_be_bytes())"),
        "rust encode must be big-endian:\n{rust}"
    );
    assert!(
        !rust.contains("from_le_bytes") && !rust.contains("to_le_bytes"),
        "u64be must not emit little-endian rust:\n{rust}"
    );

    let python = PythonEmitter.emit_page(&schema);
    assert!(python.contains("struct.unpack_from('>Q', buf, 0)"));
    assert!(python.contains("struct.pack_into('>Q', buf, 0, value)"));
    assert!(!python.contains("'<Q'"), "{python}");

    let go = GoEmitter.emit_page(&schema);
    assert!(go.contains("binary.BigEndian.Uint64(buf[0:8])"), "{go}");
    assert!(
        go.contains("binary.BigEndian.PutUint64(buf[0:], value)"),
        "{go}"
    );
    assert!(!go.contains("LittleEndian"), "{go}");

    let typescript = TypeScriptEmitter.emit_page(&schema);
    assert!(
        typescript.contains("dv.getBigUint64(0, false)"),
        "{typescript}"
    );
    assert!(
        typescript.contains("dv.setBigUint64(0, value, false)"),
        "{typescript}"
    );

    let csharp = CSharpEmitter.emit_page(&schema);
    assert!(csharp.contains("ReadUInt64BigEndian"), "{csharp}");
    assert!(csharp.contains("WriteUInt64BigEndian"), "{csharp}");
    assert!(!csharp.contains("LittleEndian"), "{csharp}");

    let java = JavaEmitter.emit_page(&schema);
    assert!(
        java.contains(
            "new java.math.BigInteger(1, new byte[] { buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7] })"
        ),
        "java decode must pass wire order as a non-negative BigInteger magnitude:\n{java}"
    );
    assert!(java.contains("java.math.BigInteger"), "{java}");
    assert!(
        !java.contains("(long)"),
        "u64be must not decode or encode as a signed long:\n{java}"
    );

    let php = PhpEmitter.emit_page(&schema);
    assert!(
        php.contains("sprintf('%u', unpack('J', substr($buf, 0, 8))[1])"),
        "{php}"
    );
    assert!(php.contains("18446744073709551615"), "{php}");
    assert!(
        !php.contains("pack('P'") && !php.contains("unpack('P'"),
        "{php}"
    );

    let openapi = OpenApiEmitter.emit_page(&schema);
    assert!(openapi.contains("\"type\": \"integer\""), "{openapi}");
    assert!(openapi.contains("\"minimum\": 0"), "{openapi}");
}

#[test]
fn c_cmis_page34h_counter_decodes_msb_first() {
    let expected = msb_first(&COUNTER_BYTES);
    assert_eq!(
        expected, 0x0102,
        "spec MSB-first reading of 00 00 00 00 00 00 01 02"
    );

    let namespaces = [
        "c-cmis-1.0",
        "c-cmis-1.1",
        "c-cmis-1.2",
        "c-cmis-1.3",
        "c-cmis-1.4",
        "c-cmis-1.5",
    ];
    for namespace in namespaces {
        let path = schemas_root()
            .join(namespace)
            .join("page-34h-media-lane-fec-pm.yaml");
        let schema =
            load_page_schema(&path).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
        validate_pages(std::slice::from_ref(&schema))
            .unwrap_or_else(|err| panic!("validate {namespace}: {err}"));
        for name in [
            "rx_bits_pm",
            "rx_bits_sub_int_pm",
            "rx_corr_bits_pm",
            "rx_min_corr_bits_sub_int_pm",
            "rx_max_corr_bits_sub_int_pm",
        ] {
            let field = schema
                .fields
                .iter()
                .find(|field| field.name == name)
                .unwrap_or_else(|| panic!("{namespace} missing {name}"));
            assert_eq!(field.ty, FieldType::U64Be, "{namespace}.{name}");
        }
        // Page 3Ah prints its counters as "U64." with no byte order; the CMIS
        // default storage order (big-endian) applies, as Page 34h states.
        let host = schemas_root()
            .join(namespace)
            .join("page-3ah-data-path-host-if-pm.yaml");
        let host =
            load_page_schema(&host).unwrap_or_else(|err| panic!("parse {}: {err}", host.display()));
        let host_counters: Vec<_> = host
            .fields
            .iter()
            .filter(|field| field.size_bytes == Some(8) && field.name.ends_with("_pm"))
            .collect();
        assert_eq!(
            host_counters.len(),
            5,
            "{namespace} page 3Ah 64-bit counters"
        );
        for field in host_counters {
            assert_eq!(
                field.ty,
                FieldType::U64Be,
                "{namespace} page 3Ah {}",
                field.name
            );
        }
        let emitted = emit_rust_page(&schema);
        assert!(
            emitted.contains("rx_bits_pm: u64::from_be_bytes(["),
            "{namespace} emitter is not MSB-first:\n{emitted}"
        );
        if namespace == "c-cmis-1.4" {
            let decoded = compile_and_decode(&emitted);
            assert_eq!(decoded, expected);
        }
    }
}

/// Compile the emitted Page 34h decoder and run it on the spec's byte
/// pattern. The expected value is computed by the caller from the
/// MSB-first rule.
fn compile_and_decode(emitted: &str) -> u64 {
    let dir = tempfile::tempdir().expect("tempdir");
    let page = emitted.replacen("use super::*;\n", "", 1);
    std::fs::write(dir.path().join("page.rs"), page).unwrap();
    let struct_name = emitted
        .lines()
        .find_map(|line| line.strip_prefix("pub struct "))
        .and_then(|rest| rest.split([' ', '{']).next())
        .expect("emitted page struct");
    let main = format!(
        r#"
#[path = "page.rs"]
mod page;
fn main() {{
    let mut buf = [0u8; 128];
    buf[..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02]);
    let decoded = page::{struct_name}::decode(&buf);
    println!("{{}}", decoded.rx_bits_pm);
}}
"#
    );
    std::fs::write(dir.path().join("main.rs"), main).unwrap();
    let bin = dir.path().join("decode-page34h");
    let compile = Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(dir.path().join("main.rs"))
        .output()
        .expect("rustc");
    assert!(
        compile.status.success(),
        "emitted page 34h decoder failed to compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&bin).output().expect("run decoder");
    assert!(
        run.status.success(),
        "decoder failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout)
        .trim()
        .parse()
        .unwrap_or_else(|err| panic!("decode output {:?}: {err}", run.stdout))
}

fn round_trip_le_page() -> openxcvr_codegen::ir::pages::PageSchema {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: U64LeRoundTrip
title: u64le round trip
page_address: lower
size_bytes: 8
fields:
  - { name: counter, offset: 0, size_bytes: 8, type: u64le, access: rw }
"#;
    let schema = parse_page_schema(yaml).expect("u64le parses");
    validate_pages(std::slice::from_ref(&schema)).expect("u64le validates");
    schema
}

/// Writable round-trip pages executed by the Java, C# and PHP binding suites.
/// CMIS Page 14h counters are read-only, so the generator emits no setter for
/// them; these fixtures are the emitter output for a writable scalar of each
/// byte order and must stay byte-identical to a fresh emit.
#[test]
fn unsigned_u64_round_trip_fixtures_match_emitter() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases = [
        (
            "bindings/java/src/test/java/com/openxcvr/generated/U64LeRoundTrip.java",
            JavaEmitter.emit_page(&round_trip_le_page()),
        ),
        (
            "bindings/java/src/test/java/com/openxcvr/generated/U64BeRoundTrip.java",
            JavaEmitter.emit_page(&round_trip_page()),
        ),
        (
            "bindings/csharp/tests/OpenXcvr.Transceivers.Tests/U64LeRoundTrip.cs",
            CSharpEmitter.emit_page(&round_trip_le_page()),
        ),
        (
            "bindings/csharp/tests/OpenXcvr.Transceivers.Tests/U64BeRoundTrip.cs",
            CSharpEmitter.emit_page(&round_trip_page()),
        ),
        (
            "bindings/php/tests/support/U64LeRoundTrip.php",
            PhpEmitter.emit_page(&round_trip_le_page()),
        ),
        (
            "bindings/php/tests/support/U64BeRoundTrip.php",
            PhpEmitter.emit_page(&round_trip_page()),
        ),
    ];
    for (path, emitted) in cases {
        let full = root.join(path);
        let committed = std::fs::read_to_string(&full)
            .unwrap_or_else(|err| panic!("read {}: {err}", full.display()));
        assert_eq!(committed, emitted, "{path} drifted from the emitter");
    }
}

#[test]
fn cmis_page14h_counters_stay_little_endian() {
    // CMIS Page 14h prints its error counters as "U64 Little-endian (LSB
    // first)"; the C-CMIS big-endian retype must not reach them.
    let path = schemas_root().join("cmis/page-14h-diag-results.yaml");
    let schema =
        load_page_schema(&path).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
    let wide: Vec<_> = schema
        .fields
        .iter()
        .filter(|field| matches!(field.ty, FieldType::U64Le | FieldType::U64Be))
        .collect();
    assert_eq!(wide.len(), 16, "CMIS Page 14h 64-bit counters");
    for field in wide {
        assert_eq!(field.ty, FieldType::U64Le, "CMIS Page 14h {}", field.name);
    }
}
