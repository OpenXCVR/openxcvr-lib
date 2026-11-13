mod enums;
pub mod media_interface;
mod pages;

pub use media_interface::emit_media_interface;

use crate::emitters::Emitter;
use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;

pub struct TypeScriptEmitter;

impl Emitter for TypeScriptEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        enums::emit_enum(schema)
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        pages::emit_page(schema)
    }
}
