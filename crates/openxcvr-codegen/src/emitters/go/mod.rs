mod enums;
pub mod media_interface;
mod pages;

pub use media_interface::emit_media_interface;

use crate::emitters::Emitter;
use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;

/// Format Go code via `gofmt` if available on the system PATH.
/// If `gofmt` is unavailable or fails, returns the unformatted code.
pub(crate) fn format_go(code: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new("gofmt")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return code.to_string();
    };

    let Some(mut stdin) = child.stdin.take() else {
        return code.to_string();
    };

    if stdin.write_all(code.as_bytes()).is_err() {
        return code.to_string();
    }
    drop(stdin);

    if let Ok(output) = child.wait_with_output() {
        if output.status.success() {
            if let Ok(formatted) = String::from_utf8(output.stdout) {
                return formatted;
            }
        }
    }

    code.to_string()
}

pub struct GoEmitter;

impl Emitter for GoEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        enums::emit_enum(schema)
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        pages::emit_page(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_go() {
        let unformatted = "package foo\ntype Bar struct {\n\tA int\n\tBBBBBB int\n}\n";
        let formatted = format_go(unformatted);
        assert_eq!(
            formatted,
            "package foo\n\ntype Bar struct {\n\tA      int\n\tBBBBBB int\n}\n"
        );
    }
}
