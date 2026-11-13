mod enums;
pub mod media_interface;
mod pages;
mod revision;

pub use media_interface::emit_media_interface;
pub use revision::emit_checked_page;

use crate::emitters::Emitter;
use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;

pub struct PythonEmitter;

impl Emitter for PythonEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        enums::emit_enum(schema)
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        pages::emit_page(schema)
    }
}
